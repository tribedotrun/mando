//! Artifact API routes -- evidence + work summary CRUD, media serving.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use mando_types::artifact::{ArtifactMedia, ArtifactType};

#[derive(Deserialize)]
pub(crate) struct RemoteUrlPatch {
    pub index: u32,
    pub remote_url: String,
}

use crate::response::{error_response, internal_error};
use crate::AppState;

fn resolve_id(id: &str, label: &str) -> Result<i64, (StatusCode, Json<Value>)> {
    id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid {label} id: {id}"),
        )
    })
}

// ── POST /api/tasks/{id}/evidence ───────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct EvidenceFileInput {
    pub filename: String,
    pub ext: String,
    pub caption: String,
}

#[derive(Deserialize)]
pub(crate) struct PostEvidenceBody {
    pub files: Vec<EvidenceFileInput>,
}

/// Register evidence artifact -- metadata only. CLI copies files to disk
/// using the returned artifact_id, then the media local_paths are deterministic.
pub(crate) async fn post_task_evidence(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PostEvidenceBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task_id = resolve_id(&id, "task")?;

    if body.files.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "at least one file required",
        ));
    }

    let pool = state.db.pool();

    // Insert row first to get the artifact_id.
    let content = format!("Evidence ({} files)", body.files.len());
    let artifact_id =
        mando_db::queries::artifacts::insert(pool, task_id, ArtifactType::Evidence, &content, &[])
            .await
            .map_err(internal_error)?;

    // Build media JSON with deterministic local_path.
    let media: Vec<ArtifactMedia> = body
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| ArtifactMedia {
            index: i as u32,
            filename: f.filename.clone(),
            ext: f.ext.clone(),
            local_path: Some(format!("artifacts/{task_id}/{artifact_id}-{i}.{}", f.ext)),
            remote_url: None,
            caption: Some(f.caption.clone()),
        })
        .collect();

    // Update artifact with media.
    mando_db::queries::artifacts::update_media(pool, artifact_id, &media)
        .await
        .map_err(internal_error)?;

    // Emit event so Electron feed updates.
    state.bus.send(
        mando_types::BusEvent::Artifacts,
        Some(json!({"action": "evidence_created", "task_id": task_id, "artifact_id": artifact_id})),
    );

    Ok(Json(json!({
        "artifact_id": artifact_id,
        "task_id": task_id,
        "media": media,
    })))
}

// ── POST /api/tasks/{id}/summary ────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct PostSummaryBody {
    pub content: String,
}

/// Save a work summary artifact (diagram + "What changed").
pub(crate) async fn post_task_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PostSummaryBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task_id = resolve_id(&id, "task")?;
    let pool = state.db.pool();

    let artifact_id = mando_db::queries::artifacts::insert(
        pool,
        task_id,
        ArtifactType::WorkSummary,
        &body.content,
        &[],
    )
    .await
    .map_err(internal_error)?;

    state.bus.send(
        mando_types::BusEvent::Artifacts,
        Some(json!({"action": "summary_created", "task_id": task_id, "artifact_id": artifact_id})),
    );

    Ok(Json(json!({
        "artifact_id": artifact_id,
        "task_id": task_id,
    })))
}

// ── GET /api/artifacts/{id}/media/{index} ───────────────────────────

/// Serve a local media file with correct Content-Type.
/// Required because Electron CSP blocks file:// URLs.
pub(crate) async fn get_artifact_media(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, String)>,
) -> Result<axum::response::Response<Body>, (StatusCode, Json<Value>)> {
    let artifact_id = resolve_id(&id, "artifact")?;
    let media_index: usize = index
        .parse()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid media index"))?;

    let pool = state.db.pool();
    let artifact = mando_db::queries::artifacts::get(pool, artifact_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "artifact not found"))?;

    let media_item = artifact
        .media
        .get(media_index)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "media index out of range"))?;

    let local_path = media_item
        .local_path
        .as_ref()
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "no local file for this media"))?;

    let data_dir = mando_types::data_dir();
    let file_path = data_dir.join(local_path);

    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|_| error_response(StatusCode::NOT_FOUND, "media file not found on disk"))?;

    let content_type = match media_item.ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(body)
        .unwrap())
}

// ── PUT /api/artifacts/{id}/media ───────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct PutMediaBody {
    pub media: Vec<RemoteUrlPatch>,
}

/// Backfill `remote_url` on existing media entries after GCS upload.
/// Only `remote_url` is writable — filename, ext, and local_path are
/// set at creation time and must not be mutable (path-traversal guard).
pub(crate) async fn put_artifact_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PutMediaBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let artifact_id = resolve_id(&id, "artifact")?;
    let pool = state.db.pool();

    let artifact = mando_db::queries::artifacts::get(pool, artifact_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "artifact not found"))?;

    let mut merged: Vec<ArtifactMedia> = artifact.media.clone();
    for patch in &body.media {
        let slot = merged
            .iter_mut()
            .find(|m| m.index == patch.index)
            .ok_or_else(|| {
                error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("media index {} not found on artifact", patch.index),
                )
            })?;
        slot.remote_url = Some(patch.remote_url.clone());
    }

    mando_db::queries::artifacts::update_media(pool, artifact_id, &merged)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({ "ok": true })))
}
