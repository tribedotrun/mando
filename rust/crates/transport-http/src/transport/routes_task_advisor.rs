//! Task advisor route handlers -- persistent per-task advisor sessions.
//!
//! This is intentionally separate from `routes_task_ask` (POST `/api/tasks/{id}/ask`).
//! The two endpoints share a near-identical task-header preamble (extracted into
//! the `_task_header` Jinja partial in `captain-workflow.yaml`) but back distinct
//! UI surfaces and persist independent histories:
//!
//! - **ask** (`session_ids.ask`, persisted as `ask_history` rows): Q&A in the
//!   task detail view (`TaskDetailView` / `TaskComposer`). Read-only assistant
//!   that answers free-form questions about the task.
//! - **advisor** (`session_ids.advisor`): the feed view's advisor button
//!   (`TaskFeedView`). Same Q&A capability, plus action-synthesis verdicts
//!   that feed `reopen` / `rework` / `revise-plan` transitions directly.
//!
//! Consolidating into a single endpoint would require migrating both
//! persistence schemes and merging two distinct UI flows -- out of scope for
//! prompt-template hygiene. The two endpoints are documented as a deliberate
//! split; if a future refactor wants to unify them, it must also unify the
//! `cc_sessions` rows and the renderer history queries.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use super::routes_task_advisor_action::post_task_advisor_action;
use super::routes_task_advisor_helpers::build_advisor_prompt;
use crate::response::{
    broadcast_task_update, error_response, internal_error, resolve_task_cwd,
    touch_workbench_activity, ApiError,
};
use crate::AppState;

const PENDING_SESSION: &str = "pending";

/// Pick the `session_id` value to persist on the advisor question row.
///
/// Resume case (`should_resume = true`): both the in-memory session manager
/// and the task carry the same id; persisting it directly avoids the
/// post-hoc backfill round-trip and keeps history coherent. Start-new case
/// (`should_resume = false`): the row gets `'pending'` and the success path
/// backfills it once the CC call returns the real id.
///
/// Gating on `should_resume` (not just on `existing.is_some()`) is the
/// invariant that matters: the route's stale-session branch (`!mgr_has_session
/// && task_has_session`) clears the disk state but not the in-memory `item`,
/// so reading the existing id directly would persist a stale id that the
/// backfill cannot touch (it only updates `'pending'` rows).
fn pick_initial_session_id(should_resume: bool, existing: Option<&str>) -> &str {
    if should_resume {
        existing.unwrap_or(PENDING_SESSION)
    } else {
        PENDING_SESSION
    }
}

/// POST /api/tasks/{id}/advisor -- send a message to the task's advisor.
///
/// - `ask` intent: conversational Q&A. Lazily spawns a CC session on first
///   message, resumes the same session for follow-ups, and persists the
///   assistant reply as an `ask_history` `assistant` row.
/// - `reopen` / `rework` / `revise-plan`: single synthesis call whose output
///   feeds directly into the transition. No conversational answer is stored,
///   so the feed shows only the user's message plus the resulting timeline
///   event (HumanReopen etc.). Prevents confusing "Want me to draft...?"
///   phrasing when the action has already been chosen.
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/advisor")]
pub(crate) async fn post_task_advisor(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
    Json(body): Json<api_types::AdvisorRequest>,
) -> Result<Json<api_types::AdvisorResponse>, ApiError> {
    if body.message.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "message must not be empty",
        ));
    }

    let workflow = state.settings.load_captain_workflow();

    let item = state
        .captain
        .load_task(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| {
            error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
        })?;

    let cwd = resolve_task_cwd(&item, &state)?;
    let session_key = format!("advisor:{task_id}");
    let sessions = state.sessions.clone();

    let mgr_has_session = sessions.has_session(&session_key);
    let task_has_session = item.session_ids.advisor.is_some();
    let should_resume = mgr_has_session && task_has_session;

    if mgr_has_session && !task_has_session {
        tracing::info!(
            module = "transport-http-transport-routes_task_advisor",
            task_id,
            "session_ids.advisor cleared -- closing stale session"
        );
        crate::runtime::task_sessions::close_advisor_session(&state, task_id).await;
    } else if !mgr_has_session && task_has_session {
        tracing::warn!(
            module = "transport-http-transport-routes_task_advisor",
            task_id,
            "stale session_ids.advisor -- clearing"
        );
        crate::runtime::task_sessions::clear_advisor_session(&state, task_id).await;
    }

    let ask_id = global_infra::uuid::Uuid::v4().to_string();

    let initial_session_id =
        pick_initial_session_id(should_resume, item.session_ids.advisor.as_deref());

    state
        .captain
        .persist_task_question(task_id, &ask_id, initial_session_id, &body.message)
        .await
        .map_err(|e| internal_error(e, "failed to persist advisor question"))?;
    broadcast_task_update(&state, task_id).await;

    if matches!(body.intent.as_str(), "reopen" | "rework" | "revise-plan") {
        return post_task_advisor_action(
            &state,
            &sessions,
            &session_key,
            should_resume,
            &item,
            &workflow,
            task_id,
            &cwd,
            &ask_id,
            &body.intent,
            &body.message,
        )
        .await;
    }

    let result = run_advisor_cc(
        &state,
        &sessions,
        &session_key,
        should_resume,
        &body.message,
        &item,
        &workflow,
        task_id,
        &ask_id,
        &cwd,
    )
    .await?;

    let answer = result.text.clone();
    let session_id = result.session_id.clone();

    if let Err(e) = state
        .captain
        .set_task_advisor_session(task_id, Some(session_id.clone()))
        .await
    {
        tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to persist session_ids.advisor");
    }

    state
        .captain
        .persist_task_answer(
            task_id,
            &ask_id,
            &session_id,
            &body.message,
            &answer,
            &body.intent,
        )
        .await
        .map_err(|e| internal_error(e, "failed to persist advisor answer"))?;

    if let Err(e) = state
        .captain
        .backfill_ask_pending_session_id(task_id, &ask_id, &session_id)
        .await
    {
        tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to backfill advisor pending session id");
    }

    broadcast_task_update(&state, task_id).await;
    touch_workbench_activity(&state, item.workbench_id).await;

    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(&state, &config);
    let mut preview: String = answer.chars().take(200).collect();
    if answer.chars().count() > 200 {
        preview.push_str("...");
    }
    let msg = format!(
        "Advisor answered on <b>{}</b>: {}",
        global_infra::html::escape_html(&item.title),
        global_infra::html::escape_html(&preview),
    );
    notifier
        .notify_typed(
            &msg,
            api_types::NotifyLevel::Normal,
            api_types::NotificationKind::AdvisorAnswered {
                item_id: task_id.to_string(),
                title: item.title.clone(),
            },
            Some(&task_id.to_string()),
        )
        .await;

    Ok(Json(api_types::AdvisorResponse::Ask(
        api_types::AdvisorAskResponse {
            id: task_id,
            ask_id,
            message: body.message,
            answer,
            session_id,
        },
    )))
}

/// Run the advisor CC session with up to max_retries attempts.
/// Each failure is persisted to ask_history so it surfaces in the feed.
#[allow(clippy::too_many_arguments)]
async fn run_advisor_cc(
    state: &AppState,
    sessions: &::sessions::SessionsRuntime,
    session_key: &str,
    should_resume: bool,
    message: &str,
    item: &captain::Task,
    workflow: &settings::CaptainWorkflow,
    task_id: i64,
    ask_id: &str,
    cwd: &std::path::Path,
) -> Result<sessions::SessionAiResult, ApiError> {
    let task_id_str = task_id.to_string();
    let timeline_text = state
        .captain
        .build_task_timeline_text(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to build advisor timeline"))?;
    let prompt = build_advisor_prompt(item, &task_id_str, message, "ask", workflow, &timeline_text)
        .map_err(|e| internal_error(e, "failed to build advisor prompt"))?;

    let max_retries = workflow.agent.max_advisor_retries;
    let mut should_resume_attempt = should_resume;

    for attempt in 1..=max_retries {
        let result = if should_resume_attempt {
            sessions
                .follow_up(::sessions::SessionFollowUpRequest {
                    key: session_key.to_string(),
                    message: message.to_string(),
                    cwd: cwd.to_path_buf(),
                })
                .await
        } else {
            sessions
                .start_with_item(::sessions::SessionStartRequest {
                    key: session_key.to_string(),
                    prompt: prompt.clone(),
                    cwd: cwd.to_path_buf(),
                    model: Some(workflow.models.captain.clone()),
                    idle_ttl: workflow.agent.task_ask_idle_ttl_s,
                    call_timeout: workflow.agent.task_ask_timeout_s,
                    task_id: Some(task_id),
                    max_turns: None,
                })
                .await
        };

        match result {
            Ok(r) => return Ok(r),
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!(
                    module = "transport-http-transport-routes_task_advisor", task_id, attempt, max_retries = max_retries,
                    error = %error_msg, "advisor CC session failed"
                );

                crate::runtime::task_sessions::clear_advisor_session(state, task_id).await;

                let display_msg = if attempt < max_retries {
                    format!("Attempt {attempt}/{max_retries} failed: {error_msg} — retrying…")
                } else {
                    format!("Failed after {max_retries} attempts: {error_msg}")
                };
                if let Err(e) = state
                    .captain
                    .persist_task_error(task_id, ask_id, PENDING_SESSION, message, &display_msg)
                    .await
                {
                    tracing::error!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to persist advisor retry error");
                }
                broadcast_task_update(state, task_id).await;

                should_resume_attempt = false;
            }
        }
    }

    Err(error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "advisor session failed after all retries",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for codex P1: when the in-memory session manager has
    /// dropped the advisor session but the task still carries
    /// `session_ids.advisor`, the route's stale-session branch clears it
    /// on disk but cannot reach into the local `item`. Picking the
    /// persisted id off `item` would write a stale uuid that the backfill
    /// (`session_id = 'pending'`) would then refuse to touch. Gating on
    /// `should_resume` is the only safe path. PR #1035.
    #[test]
    fn start_new_uses_pending_even_when_existing_session_id_is_set() {
        assert_eq!(
            pick_initial_session_id(false, Some("stale-advisor-uuid")),
            PENDING_SESSION
        );
    }

    #[test]
    fn start_new_uses_pending_when_existing_is_none() {
        assert_eq!(pick_initial_session_id(false, None), PENDING_SESSION);
    }

    #[test]
    fn resume_uses_existing_session_id() {
        assert_eq!(
            pick_initial_session_id(true, Some("real-advisor-uuid")),
            "real-advisor-uuid"
        );
    }

    #[test]
    fn resume_with_no_existing_falls_back_to_pending() {
        assert_eq!(pick_initial_session_id(true, None), PENDING_SESSION);
    }
}
