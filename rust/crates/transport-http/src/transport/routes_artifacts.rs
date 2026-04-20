//! Artifact API routes -- evidence + work summary CRUD, media serving.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::Json;
use tokio_util::io::ReaderStream;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

fn resolve_id(id: &str, label: &str) -> Result<i64, ApiError> {
    id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid {label} id: {id}"),
        )
    })
}

// ── POST /api/tasks/{id}/evidence ───────────────────────────────────

/// Register evidence artifact -- metadata only. CLI copies files to disk
/// using the returned artifact_id, then the media local_paths are deterministic.
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/evidence")]
pub(crate) async fn post_task_evidence(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
    Json(body): Json<api_types::TaskEvidenceRequest>,
) -> Result<Json<api_types::TaskEvidenceResponse>, ApiError> {
    if body.files.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "at least one file required",
        ));
    }

    let files: Vec<captain::EvidenceFileSpec> = body
        .files
        .iter()
        .map(|file| captain::EvidenceFileSpec {
            filename: file.filename.clone(),
            ext: file.ext.clone(),
            caption: file.caption.clone(),
        })
        .collect();
    let created = state
        .captain
        .create_evidence_artifact(task_id, &files)
        .await
        .map_err(|e| internal_error(e, "failed to create evidence artifact"))?;

    Ok(Json(api_types::TaskEvidenceResponse {
        artifact_id: created.artifact_id,
        task_id,
        media: serde_json::from_value(
            serde_json::to_value(created.media)
                .map_err(|e| internal_error(e, "failed to serialize evidence media"))?,
        )
        .map_err(|e| internal_error(e, "failed to decode evidence media"))?,
    }))
}

// ── POST /api/tasks/{id}/summary ────────────────────────────────────

/// Save a work summary artifact (diagram + "What changed").
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/summary")]
pub(crate) async fn post_task_summary(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
    Json(body): Json<api_types::TaskSummaryRequest>,
) -> Result<Json<api_types::TaskSummaryResponse>, ApiError> {
    let artifact_id = state
        .captain
        .create_work_summary_artifact(task_id, &body.content)
        .await
        .map_err(|e| internal_error(e, "failed to create work summary"))?;

    Ok(Json(api_types::TaskSummaryResponse {
        artifact_id,
        task_id,
    }))
}

// ── GET /api/artifacts/{id}/media/{index} ───────────────────────────

/// Serve a local media file with correct Content-Type.
/// Required because Electron CSP blocks file:// URLs.
#[crate::instrument_api(method = "GET", path = "/api/artifacts/{id}/media/{index}")]
pub(crate) async fn get_artifact_media(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, String)>,
) -> Result<axum::response::Response<Body>, ApiError> {
    let artifact_id = resolve_id(&id, "artifact")?;
    let media_index: usize = index
        .parse()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid media index"))?;

    let artifact = state
        .captain
        .get_artifact(artifact_id)
        .await
        .map_err(|e| internal_error(e, "failed to load artifact"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "artifact not found"))?;

    let media_item = artifact
        .media
        .get(media_index)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "media index out of range"))?;

    let local_path = media_item
        .local_path
        .as_ref()
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "no local file for this media"))?;

    let data_dir = global_types::data_dir();
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

    match axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(body)
    {
        Ok(resp) => Ok(resp),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to build artifact response: {e}"),
        )),
    }
}

// ── PUT /api/artifacts/{id}/media ───────────────────────────────────

/// Backfill `remote_url` on existing media entries after GCS upload.
/// Only `remote_url` is writable — filename, ext, and local_path are
/// set at creation time and must not be mutable (path-traversal guard).
#[crate::instrument_api(method = "PUT", path = "/api/artifacts/{id}/media")]
pub(crate) async fn put_artifact_media(
    State(state): State<AppState>,
    Path(api_types::ArtifactIdParams { id: artifact_id }): Path<api_types::ArtifactIdParams>,
    Json(body): Json<api_types::ArtifactMediaUpdateRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let patches: Vec<(u32, String)> = body
        .media
        .iter()
        .map(|patch| (patch.index, patch.remote_url.clone()))
        .collect();
    match state
        .captain
        .update_artifact_media(artifact_id, &patches)
        .await
        .map_err(|e| internal_error(e, "failed to update artifact media"))?
    {
        captain::UpdateArtifactMediaOutcome::Updated => {
            Ok(Json(api_types::BoolOkResponse { ok: true }))
        }
        captain::UpdateArtifactMediaOutcome::ArtifactNotFound => {
            Err(error_response(StatusCode::NOT_FOUND, "artifact not found"))
        }
        captain::UpdateArtifactMediaOutcome::MediaIndexNotFound(index) => Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("media index {} not found on artifact", index),
        )),
    }
}
