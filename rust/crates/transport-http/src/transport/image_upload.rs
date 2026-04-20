//! Shared image-upload helpers for multipart endpoints.
//!
//! Provides dual-mode extraction: multipart for image-capable clients
//! (Electron), JSON for text-only clients (CLI, Telegram).

use axum::extract::multipart::Field;
use axum::extract::{FromRequest, Multipart};
use axum::http::{header, StatusCode};

use crate::response::{error_response, internal_error_with, ApiError};

// ── Parsed request bodies ──────────────────────────────────────────────

/// Feedback body (reopen/rework) parsed from JSON or multipart.
pub(crate) struct FeedbackWithImages {
    pub id: i64,
    pub feedback: String,
    pub saved_images: Vec<String>,
}

/// Ask body parsed from JSON or multipart.
pub(crate) struct AskWithImages {
    pub id: i64,
    pub question: String,
    pub ask_id: Option<String>,
    pub saved_images: Vec<String>,
}

/// Scout ask body parsed from JSON or multipart.
pub(crate) struct ScoutAskWithImages {
    pub id: i64,
    pub question: String,
    pub session_id: Option<String>,
    pub saved_images: Vec<String>,
}

/// Nudge body parsed from JSON or multipart.
pub(crate) struct NudgeWithImages {
    pub item_id: String,
    pub message: String,
    pub saved_images: Vec<String>,
}

/// Clarify body parsed from JSON or multipart.
pub(crate) struct ClarifyWithImages {
    pub answers: Option<Vec<ClarifyQA>>,
    pub answer: Option<String>,
    pub saved_images: Vec<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ClarifyQA {
    pub question: String,
    pub answer: String,
}

// ── Image disk I/O ─────────────────────────────────────────────────────

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Save a multipart image field to `~/.mando/images/`. Returns the filename.
pub(crate) async fn save_image_field(field: Field<'_>) -> Result<String, ApiError> {
    let images_dir = global_infra::paths::images_dir();
    let filename = field.file_name().unwrap_or("upload").to_string();
    let ext = filename
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 5)
        .unwrap_or("bin");
    let uuid = global_infra::uuid::Uuid::v4();
    let dest_name = format!("{uuid}.{ext}");

    let data = field
        .bytes()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
    if data.len() > MAX_IMAGE_BYTES {
        return Err(error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "image exceeds 10 MB limit",
        ));
    }

    tokio::fs::create_dir_all(&images_dir).await.map_err(|e| {
        internal_error_with(StatusCode::INTERNAL_SERVER_ERROR, e, "failed to save image")
    })?;
    tokio::fs::write(images_dir.join(&dest_name), &data)
        .await
        .map_err(|e| {
            internal_error_with(StatusCode::INTERNAL_SERVER_ERROR, e, "failed to save image")
        })?;

    Ok(dest_name)
}

/// Best-effort cleanup of saved image files (e.g. when a handler fails
/// after multipart parsing). Logs but does not propagate errors.
pub(crate) async fn cleanup_saved_images(filenames: &[String]) {
    if filenames.is_empty() {
        return;
    }
    let dir = global_infra::paths::images_dir();
    for name in filenames {
        let path = dir.join(name);
        if let Err(e) = tokio::fs::remove_file(&path).await {
            tracing::warn!(module = "transport-http-transport-image_upload", file = %path.display(), error = %e, "failed to clean up orphaned image");
        }
    }
}

/// Format image filenames as absolute paths for worker prompts.
pub(crate) fn format_image_paths(filenames: &[String]) -> String {
    if filenames.is_empty() {
        return String::new();
    }
    let dir = global_infra::paths::images_dir();
    let lines: Vec<String> = filenames
        .iter()
        .map(|f| format!("- {}", dir.join(f).display()))
        .collect();
    format!("\n\n## Attached Images\n{}", lines.join("\n"))
}

// ── Shared multipart helpers ───────────────────────────────────────────

pub(crate) fn is_multipart(request: &axum::extract::Request) -> bool {
    request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("multipart/"))
}

pub(crate) async fn into_multipart(request: axum::extract::Request) -> Result<Multipart, ApiError> {
    <Multipart as FromRequest<()>>::from_request(request, &())
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))
}

pub(crate) async fn field_text(field: Field<'_>) -> Result<String, ApiError> {
    field
        .text()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))
}

pub(crate) async fn field_id(field: Field<'_>) -> Result<i64, ApiError> {
    let text = field_text(field).await?;
    text.parse()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid id"))
}

// ── Feedback (reopen/rework) ───────────────────────────────────────────

/// Extract `FeedbackWithImages` from either JSON or multipart.
pub(crate) async fn extract_feedback(
    request: axum::extract::Request,
) -> Result<FeedbackWithImages, ApiError> {
    if is_multipart(&request) {
        extract_feedback_multipart(request).await
    } else {
        extract_feedback_json(request).await
    }
}

async fn extract_feedback_json(
    request: axum::extract::Request,
) -> Result<FeedbackWithImages, ApiError> {
    let body = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    #[derive(Default, serde::Deserialize)]
    #[serde(default)]
    struct Body {
        id: i64,
        feedback: String,
    }
    let b: Body = serde_json::from_slice(&body)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    Ok(FeedbackWithImages {
        id: b.id,
        feedback: b.feedback,
        saved_images: Vec::new(),
    })
}

async fn extract_feedback_multipart(
    request: axum::extract::Request,
) -> Result<FeedbackWithImages, ApiError> {
    let mut mp = into_multipart(request).await?;
    let mut id: Option<i64> = None;
    let mut feedback = String::new();
    let mut saved = Vec::new();

    let result = extract_feedback_fields(&mut mp, &mut id, &mut feedback, &mut saved).await;
    if let Err(e) = result {
        cleanup_saved_images(&saved).await;
        return Err(e);
    }
    let id = match id {
        Some(v) => v,
        None => {
            cleanup_saved_images(&saved).await;
            return Err(error_response(StatusCode::BAD_REQUEST, "id is required"));
        }
    };
    Ok(FeedbackWithImages {
        id,
        feedback,
        saved_images: saved,
    })
}

async fn extract_feedback_fields(
    mp: &mut Multipart,
    id: &mut Option<i64>,
    feedback: &mut String,
    saved: &mut Vec<String>,
) -> Result<(), ApiError> {
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or("") {
            "id" => *id = Some(field_id(field).await?),
            "feedback" => *feedback = field_text(field).await?,
            "images" => saved.push(save_image_field(field).await?),
            _ => {}
        }
    }
    Ok(())
}

// ── Ask ────────────────────────────────────────────────────────────────

/// Extract `AskWithImages` from either JSON or multipart.
pub(crate) async fn extract_ask(
    request: axum::extract::Request,
) -> Result<AskWithImages, ApiError> {
    if is_multipart(&request) {
        extract_ask_multipart(request).await
    } else {
        extract_ask_json(request).await
    }
}

async fn extract_ask_json(request: axum::extract::Request) -> Result<AskWithImages, ApiError> {
    let body = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    #[derive(Default, serde::Deserialize)]
    #[serde(default)]
    struct Body {
        id: i64,
        question: String,
        ask_id: Option<String>,
    }
    let b: Body = serde_json::from_slice(&body)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    Ok(AskWithImages {
        id: b.id,
        question: b.question,
        ask_id: b.ask_id,
        saved_images: Vec::new(),
    })
}

async fn extract_ask_multipart(request: axum::extract::Request) -> Result<AskWithImages, ApiError> {
    let mut mp = into_multipart(request).await?;
    let mut id: Option<i64> = None;
    let mut question = String::new();
    let mut ask_id: Option<String> = None;
    let mut saved = Vec::new();

    let result = extract_ask_fields(&mut mp, &mut id, &mut question, &mut ask_id, &mut saved).await;
    if let Err(e) = result {
        cleanup_saved_images(&saved).await;
        return Err(e);
    }
    if question.is_empty() {
        cleanup_saved_images(&saved).await;
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "question is required",
        ));
    }
    let id = match id {
        Some(v) => v,
        None => {
            cleanup_saved_images(&saved).await;
            return Err(error_response(StatusCode::BAD_REQUEST, "id is required"));
        }
    };
    Ok(AskWithImages {
        id,
        question,
        ask_id,
        saved_images: saved,
    })
}

async fn extract_ask_fields(
    mp: &mut Multipart,
    id: &mut Option<i64>,
    question: &mut String,
    ask_id: &mut Option<String>,
    saved: &mut Vec<String>,
) -> Result<(), ApiError> {
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or("") {
            "id" => *id = Some(field_id(field).await?),
            "question" => *question = field_text(field).await?,
            "ask_id" => {
                let text = field_text(field).await?;
                if !text.is_empty() {
                    *ask_id = Some(text);
                }
            }
            "images" => saved.push(save_image_field(field).await?),
            _ => {}
        }
    }
    Ok(())
}
