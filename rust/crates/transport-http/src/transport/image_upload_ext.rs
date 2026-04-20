//! Extended image-upload extractors for scout ask, nudge, and clarify endpoints.

use axum::extract::Multipart;
use axum::http::StatusCode;

use crate::image_upload::{
    cleanup_saved_images, field_id, field_text, into_multipart, is_multipart, save_image_field,
    ClarifyQA, ClarifyWithImages, NudgeWithImages, ScoutAskWithImages,
};
use crate::response::{error_response, ApiError};

// ── Scout Ask ─────────────────────────────────────────────────────────

/// Extract `ScoutAskWithImages` from either JSON or multipart.
pub(crate) async fn extract_scout_ask(
    request: axum::extract::Request,
) -> Result<ScoutAskWithImages, ApiError> {
    if is_multipart(&request) {
        extract_scout_ask_multipart(request).await
    } else {
        extract_scout_ask_json(request).await
    }
}

async fn extract_scout_ask_json(
    request: axum::extract::Request,
) -> Result<ScoutAskWithImages, ApiError> {
    let body = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    #[derive(serde::Deserialize)]
    struct Body {
        id: i64,
        question: String,
        #[serde(default)]
        session_id: Option<String>,
    }
    let b: Body = serde_json::from_slice(&body)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    Ok(ScoutAskWithImages {
        id: b.id,
        question: b.question,
        session_id: b.session_id,
        saved_images: Vec::new(),
    })
}

async fn extract_scout_ask_multipart(
    request: axum::extract::Request,
) -> Result<ScoutAskWithImages, ApiError> {
    let mut mp = into_multipart(request).await?;
    let mut id: Option<i64> = None;
    let mut question = String::new();
    let mut session_id: Option<String> = None;
    let mut saved = Vec::new();

    let result =
        extract_scout_ask_fields(&mut mp, &mut id, &mut question, &mut session_id, &mut saved)
            .await;
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
    Ok(ScoutAskWithImages {
        id,
        question,
        session_id,
        saved_images: saved,
    })
}

async fn extract_scout_ask_fields(
    mp: &mut Multipart,
    id: &mut Option<i64>,
    question: &mut String,
    session_id: &mut Option<String>,
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
            "session_id" => {
                let text = field_text(field).await?;
                if !text.is_empty() {
                    *session_id = Some(text);
                }
            }
            "images" => saved.push(save_image_field(field).await?),
            _ => {}
        }
    }
    Ok(())
}

// ── Nudge ─────────────────────────────────────────────────────────────

/// Extract `NudgeWithImages` from either JSON or multipart.
pub(crate) async fn extract_nudge(
    request: axum::extract::Request,
) -> Result<NudgeWithImages, ApiError> {
    if is_multipart(&request) {
        extract_nudge_multipart(request).await
    } else {
        extract_nudge_json(request).await
    }
}

async fn extract_nudge_json(request: axum::extract::Request) -> Result<NudgeWithImages, ApiError> {
    let body = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    let b: api_types::NudgeRequest = serde_json::from_slice(&body)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    Ok(NudgeWithImages {
        item_id: b.item_id,
        message: b.message,
        saved_images: Vec::new(),
    })
}

async fn extract_nudge_multipart(
    request: axum::extract::Request,
) -> Result<NudgeWithImages, ApiError> {
    let mut mp = into_multipart(request).await?;
    let mut item_id = String::new();
    let mut message = String::new();
    let mut saved = Vec::new();

    let result = extract_nudge_fields(&mut mp, &mut item_id, &mut message, &mut saved).await;
    if let Err(e) = result {
        cleanup_saved_images(&saved).await;
        return Err(e);
    }
    if item_id.is_empty() {
        cleanup_saved_images(&saved).await;
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "item_id is required",
        ));
    }
    if message.is_empty() {
        cleanup_saved_images(&saved).await;
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "message is required",
        ));
    }
    Ok(NudgeWithImages {
        item_id,
        message,
        saved_images: saved,
    })
}

async fn extract_nudge_fields(
    mp: &mut Multipart,
    item_id: &mut String,
    message: &mut String,
    saved: &mut Vec<String>,
) -> Result<(), ApiError> {
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or("") {
            "item_id" => *item_id = field_text(field).await?,
            "message" => *message = field_text(field).await?,
            "images" => saved.push(save_image_field(field).await?),
            _ => {}
        }
    }
    Ok(())
}

// ── Clarify ───────────────────────────────────────────────────────────

/// Extract `ClarifyWithImages` from either JSON or multipart.
pub(crate) async fn extract_clarify(
    request: axum::extract::Request,
) -> Result<ClarifyWithImages, ApiError> {
    if is_multipart(&request) {
        extract_clarify_multipart(request).await
    } else {
        extract_clarify_json(request).await
    }
}

async fn extract_clarify_json(
    request: axum::extract::Request,
) -> Result<ClarifyWithImages, ApiError> {
    let body = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(default)]
        answers: Option<Vec<ClarifyQA>>,
        #[serde(default)]
        answer: Option<String>,
    }
    let b: Body = serde_json::from_slice(&body)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

    Ok(ClarifyWithImages {
        answers: b.answers,
        answer: b.answer,
        saved_images: Vec::new(),
    })
}

async fn extract_clarify_multipart(
    request: axum::extract::Request,
) -> Result<ClarifyWithImages, ApiError> {
    let mut mp = into_multipart(request).await?;
    let mut answers: Option<Vec<ClarifyQA>> = None;
    let mut answer: Option<String> = None;
    let mut saved = Vec::new();

    let result = extract_clarify_fields(&mut mp, &mut answers, &mut answer, &mut saved).await;
    if let Err(e) = result {
        cleanup_saved_images(&saved).await;
        return Err(e);
    }
    Ok(ClarifyWithImages {
        answers,
        answer,
        saved_images: saved,
    })
}

async fn extract_clarify_fields(
    mp: &mut Multipart,
    answers: &mut Option<Vec<ClarifyQA>>,
    answer: &mut Option<String>,
    saved: &mut Vec<String>,
) -> Result<(), ApiError> {
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or("") {
            "answers" => {
                let text = field_text(field).await?;
                let parsed: Vec<ClarifyQA> = serde_json::from_str(&text)
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                *answers = Some(parsed);
            }
            "answer" => {
                let text = field_text(field).await?;
                if !text.is_empty() {
                    *answer = Some(text);
                }
            }
            "images" => saved.push(save_image_field(field).await?),
            _ => {}
        }
    }
    Ok(())
}
