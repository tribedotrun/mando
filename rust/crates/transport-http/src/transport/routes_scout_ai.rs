//! AI-powered scout route handlers: research + Q&A.

use axum::extract::{Path, State};
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

fn decode_response<T: DeserializeOwned>(
    value: Value,
    context: &'static str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|err| internal_error(err, context))
}

fn build_scout_ask_response(
    id: i64,
    question: &str,
    answer: String,
    session_id: Option<String>,
    suggested_followups: Vec<String>,
) -> api_types::AskResponse {
    api_types::AskResponse {
        id: Some(id),
        ask_id: global_infra::uuid::Uuid::v4().to_string(),
        question: Some(question.to_string()),
        answer,
        session_id,
        suggested_followups: Some(suggested_followups),
    }
}

/// POST /api/scout/research - kick off async research, return run_id immediately.
#[crate::instrument_api(method = "POST", path = "/api/scout/research")]
pub(crate) async fn post_scout_research(
    State(state): State<AppState>,
    Json(body): Json<api_types::ScoutResearchRequest>,
) -> Result<Json<api_types::ResearchStartResponse>, ApiError> {
    let run_id = state
        .scout
        .start_research(body.topic.clone(), body.process.unwrap_or(true))
        .await
        .map_err(|e| internal_error(e, "failed to create research run"))?;

    Ok(Json(api_types::ResearchStartResponse { run_id }))
}

/// GET /api/scout/research - list recent research runs.
#[crate::instrument_api(method = "GET", path = "/api/scout/research")]
pub(crate) async fn get_scout_research_runs(
    State(state): State<AppState>,
) -> Result<Json<Vec<api_types::ScoutResearchRun>>, ApiError> {
    let runs = state
        .scout
        .list_research_runs(50)
        .await
        .map_err(|e| internal_error(e, "failed to load research runs"))?;
    let value = serde_json::to_value(&runs)
        .map_err(|e| internal_error(e, "failed to serialize research runs"))?;
    Ok(Json(decode_response(
        value,
        "failed to decode research runs response",
    )?))
}

/// GET /api/scout/research/{id}/items - items discovered by a research run.
#[crate::instrument_api(method = "GET", path = "/api/scout/research/{id}/items")]
pub(crate) async fn get_scout_research_run_items(
    State(state): State<AppState>,
    Path(api_types::ScoutResearchIdParams { id }): Path<api_types::ScoutResearchIdParams>,
) -> Result<Json<Vec<api_types::ScoutItem>>, ApiError> {
    let items = state
        .scout
        .list_research_run_items(id)
        .await
        .map_err(|e| internal_error(e, "failed to load research run items"))?;
    let value = serde_json::to_value(&items)
        .map_err(|e| internal_error(e, "failed to serialize research items"))?;
    Ok(Json(decode_response(
        value,
        "failed to decode research items response",
    )?))
}

/// GET /api/scout/research/{id} - poll research run status.
#[crate::instrument_api(method = "GET", path = "/api/scout/research/{id}")]
pub(crate) async fn get_scout_research_run(
    State(state): State<AppState>,
    Path(api_types::ScoutResearchIdParams { id }): Path<api_types::ScoutResearchIdParams>,
) -> Result<Json<api_types::ScoutResearchRun>, ApiError> {
    let run = state
        .scout
        .get_research_run(id)
        .await
        .map_err(|e| internal_error(e, "failed to load research run"))?
        .ok_or_else(|| {
            error_response(
                axum::http::StatusCode::NOT_FOUND,
                &format!("research run #{id} not found"),
            )
        })?;
    let value = serde_json::to_value(&run)
        .map_err(|e| internal_error(e, "failed to serialize research run"))?;
    Ok(Json(decode_response(
        value,
        "failed to decode research run response",
    )?))
}

/// POST /api/scout/ask (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/scout/ask")]
pub(crate) async fn post_scout_ask(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::AskResponse>, ApiError> {
    let body = crate::image_upload_ext::extract_scout_ask(request).await?;
    let result = post_scout_ask_inner(&state, &body).await;
    // Scout images are ephemeral (no task.images column). Clean up after
    // the CC session has read them, regardless of success or failure.
    crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    result
}

async fn post_scout_ask_inner(
    state: &AppState,
    body: &crate::image_upload::ScoutAskWithImages,
) -> Result<Json<api_types::AskResponse>, ApiError> {
    let id = body.id;
    let session_key = body.session_id.clone();

    // Embed image paths in the question so the CC session can read them.
    let question = if body.saved_images.is_empty() {
        body.question.clone()
    } else {
        format!(
            "{}{}",
            body.question,
            crate::image_upload::format_image_paths(&body.saved_images)
        )
    };

    let qa_result = state
        .scout
        .ask_about_item(id, &question, session_key.as_deref())
        .await
        .map_err(|e| internal_error(e, "scout Q&A session failed"))?;

    // Record the Q&A session in cc_sessions.
    if let Some(ref sid) = qa_result.session_id {
        state
            .scout
            .record_qa_session(
                id,
                sid,
                qa_result.cost_usd,
                qa_result.duration_ms,
                qa_result.credential_id,
            )
            .await;
    }

    Ok(Json(build_scout_ask_response(
        id,
        &body.question,
        qa_result.answer,
        qa_result.session_id,
        qa_result.suggested_followups,
    )))
}

#[cfg(test)]
mod tests {
    use super::build_scout_ask_response;

    #[test]
    fn scout_ask_response_preserves_contract_fields() {
        let response = build_scout_ask_response(
            7,
            "what changed?",
            "here is the answer".to_string(),
            Some("sid-123".to_string()),
            vec!["next?".to_string()],
        );

        assert_eq!(response.id, Some(7));
        assert!(!response.ask_id.is_empty());
        assert_eq!(response.question.as_deref(), Some("what changed?"));
        assert_eq!(response.answer, "here is the answer");
        assert_eq!(response.session_id.as_deref(), Some("sid-123"));
        assert_eq!(
            response.suggested_followups,
            Some(vec!["next?".to_string()])
        );
    }
}
