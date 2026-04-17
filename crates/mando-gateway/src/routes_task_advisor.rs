//! Task advisor route handlers -- persistent per-task advisor sessions.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{
    broadcast_task_update, error_response, internal_error, resolve_task_cwd,
    touch_workbench_activity,
};
use crate::AppState;

const PENDING_SESSION: &str = "pending";

#[derive(Deserialize)]
pub(crate) struct AdvisorBody {
    pub message: String,
    /// Intent hint: "ask" (default), "reopen", or "rework".
    #[serde(default = "default_intent")]
    pub intent: String,
}

fn default_intent() -> String {
    "ask".into()
}

/// POST /api/tasks/{id}/advisor -- send a message to the task's advisor.
///
/// Lazy-spawns a CC session on the first message. Follow-up messages
/// resume the same session via `--resume`.
pub(crate) async fn post_task_advisor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AdvisorBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task_id: i64 = id
        .parse()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, &format!("invalid task id: {id}")))?;

    if body.message.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "message must not be empty",
        ));
    }

    let workflow = state.captain_workflow.load_full();

    // Load task + pool.
    let (item, pool) = {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(task_id)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
            })?;
        (item, store.pool().clone())
    };

    let cwd = resolve_task_cwd(&item, &state)?;
    let session_key = format!("advisor:{task_id}");
    let mgr = state.cc_session_mgr.clone();

    let mgr_has_session = mgr.has_session(&session_key);
    let task_has_session = item.session_ids.advisor.is_some();
    let should_resume = mgr_has_session && task_has_session;

    // Clean up stale state.
    if mgr_has_session && !task_has_session {
        tracing::info!(
            task_id,
            "session_ids.advisor cleared -- closing stale session"
        );
        mgr.close(&session_key);
    } else if !mgr_has_session && task_has_session {
        tracing::warn!(task_id, "stale session_ids.advisor -- clearing");
        clear_advisor_session(&state, task_id).await;
    }

    // Generate ask_id for conversation tracking.
    let ask_id = global_infra::uuid::Uuid::v4().to_string();

    // Persist user message immediately.
    captain::runtime::task_ask::persist_question(
        &pool,
        task_id,
        &ask_id,
        PENDING_SESSION,
        &body.message,
    )
    .await
    .map_err(|e| internal_error(e, "failed to persist advisor question"))?;
    broadcast_task_update(&state, task_id).await;

    // Run the CC session with retries. Each failure is surfaced in the feed.
    let result = run_advisor_cc(
        &state,
        &mgr,
        &session_key,
        should_resume,
        &body.message,
        &item,
        &workflow,
        &pool,
        task_id,
        &ask_id,
        &cwd,
    )
    .await?;

    drop(mgr);

    let answer = result.text.clone();
    let session_id = result.session_id.clone();

    // Always persist session_ids.advisor -- retries may have fallen back to a
    // fresh session even when the original intent was to resume.
    {
        let store = state.task_store.write().await;
        match store.find_by_id(task_id).await {
            Ok(Some(mut task)) => {
                task.session_ids.advisor = Some(session_id.clone());
                if let Err(e) = store.write_task(&task).await {
                    tracing::warn!(task_id, error = %e, "failed to persist session_ids.advisor");
                }
            }
            Ok(None) => tracing::warn!(task_id, "task vanished before persisting advisor session"),
            Err(e) => {
                tracing::warn!(task_id, error = %e, "failed to read task for advisor session persist")
            }
        }
    }

    // Persist answer + timeline event.
    captain::runtime::task_ask::persist_answer(
        &pool,
        task_id,
        &ask_id,
        &session_id,
        &body.message,
        &answer,
        &body.intent,
    )
    .await
    .map_err(|e| internal_error(e, "failed to persist advisor answer"))?;

    broadcast_task_update(&state, task_id).await;
    touch_workbench_activity(&state, item.workbench_id).await;

    // Notify on ask intent (action intents notify via captain state transitions).
    if body.intent == "ask" {
        let config = state.config.load_full();
        let notifier = crate::captain_notifier(&state, &config);
        let mut preview: String = answer.chars().take(200).collect();
        if answer.chars().count() > 200 {
            preview.push_str("...");
        }
        let msg = format!(
            "Advisor answered on <b>{}</b>: {}",
            transport_tg::telegram_format::escape_html(&item.title),
            transport_tg::telegram_format::escape_html(&preview),
        );
        notifier
            .notify_typed(
                &msg,
                global_types::notify::NotifyLevel::Normal,
                global_types::events::NotificationKind::AdvisorAnswered {
                    item_id: task_id.to_string(),
                    title: item.title.clone(),
                },
                Some(&task_id.to_string()),
            )
            .await;
    }

    // Handle reopen/rework/revise-plan intent.
    if body.intent == "reopen" || body.intent == "rework" || body.intent == "revise-plan" {
        let action_result = handle_advisor_action(
            &state,
            task_id,
            &session_key,
            &body.intent,
            &cwd,
            &workflow,
            &answer,
        )
        .await;

        if let Err(ref err_resp) = action_result {
            let error_msg = err_resp.1 .0["error"].as_str().unwrap_or("action failed");
            if let Err(e) = captain::runtime::task_ask::persist_error(
                &pool,
                task_id,
                &ask_id,
                PENDING_SESSION,
                &body.message,
                error_msg,
            )
            .await
            {
                tracing::error!(task_id, error = %e, "failed to persist advisor action error");
            }
            broadcast_task_update(&state, task_id).await;
        }

        return action_result;
    }

    Ok(Json(json!({
        "id": task_id,
        "ask_id": ask_id,
        "message": body.message,
        "answer": answer,
        "session_id": session_id,
    })))
}

/// Run the advisor CC session with up to max_retries attempts.
/// Each failure is persisted to ask_history so it surfaces in the feed.
#[allow(clippy::too_many_arguments)]
async fn run_advisor_cc(
    state: &AppState,
    mgr: &captain::io::cc_session::CcSessionManager,
    session_key: &str,
    should_resume: bool,
    message: &str,
    item: &captain::Task,
    workflow: &settings::config::CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    task_id: i64,
    ask_id: &str,
    cwd: &std::path::Path,
) -> Result<global_claude::CcResult, (StatusCode, Json<Value>)> {
    // Pre-build prompt so retries can start a fresh session.
    let task_id_str = task_id.to_string();
    let timeline_text = captain::runtime::task_ask::build_timeline_text(pool, task_id)
        .await
        .map_err(|e| internal_error(e, "failed to build advisor timeline"))?;
    let prompt = build_advisor_prompt(item, &task_id_str, message, workflow, &timeline_text)
        .map_err(|e| internal_error(e, "failed to build advisor prompt"))?;

    let max_retries = workflow.agent.max_advisor_retries;
    let mut should_resume_attempt = should_resume;

    for attempt in 1..=max_retries {
        let result = if should_resume_attempt {
            mgr.follow_up(session_key, message, cwd).await
        } else {
            mgr.start_with_item(
                session_key,
                &prompt,
                cwd,
                Some(&workflow.models.captain),
                workflow.agent.task_ask_idle_ttl_s,
                workflow.agent.task_ask_timeout_s,
                Some(task_id),
                None,
            )
            .await
        };

        match result {
            Ok(r) => return Ok(r),
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!(
                    task_id, attempt, max_retries = max_retries,
                    error = %error_msg, "advisor CC session failed"
                );

                mgr.close(session_key);
                clear_advisor_session(state, task_id).await;

                let display_msg = if attempt < max_retries {
                    format!("Attempt {attempt}/{max_retries} failed: {error_msg} \u{2014} retrying\u{2026}")
                } else {
                    format!("Failed after {max_retries} attempts: {error_msg}")
                };
                if let Err(e) = captain::runtime::task_ask::persist_error(
                    pool,
                    task_id,
                    ask_id,
                    PENDING_SESSION,
                    message,
                    &display_msg,
                )
                .await
                {
                    tracing::error!(task_id, error = %e, "failed to persist advisor retry error");
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

/// Handle reopen/rework via advisor synthesis.
async fn handle_advisor_action(
    state: &AppState,
    task_id: i64,
    session_key: &str,
    intent: &str,
    cwd: &std::path::Path,
    workflow: &settings::config::CaptainWorkflow,
    _initial_answer: &str,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check status eligibility BEFORE the expensive CC synthesis call.
    {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(task_id)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
            })?;
        if !action_eligible(intent, &item.status) {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                &format!("cannot {} from status {:?}", intent, item.status),
            ));
        }
    }

    let mgr = &state.cc_session_mgr;

    // Synthesize feedback via follow-up.
    let synthesis_prompt = workflow
        .prompts
        .get("advisor_reopen_synthesis")
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "advisor_reopen_synthesis prompt missing from captain-workflow.yaml",
            )
        })?
        .clone();

    let result = mgr
        .follow_up(session_key, &synthesis_prompt, cwd)
        .await
        .map_err(|e| internal_error(e, "advisor synthesis session failed"))?;
    let synthesized_feedback = result.text.clone();

    // Close any active ask session -- the worker will modify the codebase.
    crate::routes_task_ask::close_ask_session(state, task_id).await;

    // Perform the action.
    let config = state.config.load_full();
    let notifier = crate::captain_notifier(state, &config);
    let store = state.task_store.write().await;
    let mut item = store
        .find_by_id(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished during synthesis"))?;

    // Re-validate status: captain may have transitioned the task during synthesis.
    if !action_eligible(intent, &item.status) {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!(
                "task status changed to {:?} during synthesis -- cannot {}",
                item.status, intent
            ),
        ));
    }

    if intent == "revise-plan" && item.status == captain::ItemStatus::PlanReady {
        // Re-queue the planning pipeline with user feedback injected.
        let existing_ctx = item.context.as_deref().unwrap_or("");
        item.context = Some(format!(
            "{existing_ctx}\n\n## Revision feedback\n{synthesized_feedback}"
        ));
        item.planning = true;
        item.status = captain::ItemStatus::Queued;
        item.last_activity_at = Some(global_types::now_rfc3339());
        store
            .write_task(&item)
            .await
            .map_err(|e| internal_error(e, "failed to save task"))?;
    } else if intent == "rework" {
        captain::runtime::dashboard::rework_item(&store, task_id, &synthesized_feedback)
            .await
            .map_err(|e| internal_error(e, "failed to rework task"))?;
        item = store
            .find_by_id(task_id)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
            .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished after rework"))?;
    } else {
        let _outcome = captain::runtime::action_contract::reopen_item(
            &mut item,
            "human",
            &synthesized_feedback,
            &config,
            workflow,
            &notifier,
            store.pool(),
            true,
        )
        .await
        .map_err(|e| internal_error(e, "failed to reopen task"))?;
        store
            .write_task(&item)
            .await
            .map_err(|e| internal_error(e, "failed to save task"))?;
    }

    // Close advisor session after action.
    state.cc_session_mgr.close(session_key);
    {
        if let Ok(Some(mut task)) = store.find_by_id(task_id).await {
            task.session_ids.advisor = None;
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(task_id, error = %e, "failed to clear session_ids.advisor after action");
            }
        }
    }

    // Emit timeline event.
    let event_type = if intent == "rework" {
        captain::TimelineEventType::ReworkRequested
    } else {
        captain::TimelineEventType::HumanReopen
    };
    let label = if intent == "rework" {
        "Rework"
    } else {
        "Reopened"
    };
    let summary = format!("{label} from advisor: {synthesized_feedback}");
    if let Err(e) = captain::runtime::timeline_emit::emit_for_task(
        &item,
        event_type,
        &summary,
        json!({"content": &synthesized_feedback, "source": "advisor"}),
        store.pool(),
    )
    .await
    {
        tracing::warn!(task_id, error = %e, "failed to emit advisor timeline event");
    }

    state.bus.send(
        global_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": serde_json::to_value(&item).unwrap(), "id": task_id})),
    );
    touch_workbench_activity(state, item.workbench_id).await;

    Ok(Json(json!({
        "ok": true,
        "intent": intent,
        "feedback": synthesized_feedback,
    })))
}

/// Build the initial advisor prompt from the workflow template.
fn build_advisor_prompt(
    item: &captain::Task,
    task_id: &str,
    question: &str,
    workflow: &settings::config::CaptainWorkflow,
    timeline_text: &str,
) -> anyhow::Result<String> {
    use rustc_hash::FxHashMap;

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", &item.title);
    vars.insert("id", task_id);
    let status_str = item.status.as_str();
    vars.insert("status", status_str);
    vars.insert("project", &item.project);
    let pr = item.pr_number.map(|n| n.to_string()).unwrap_or_default();
    vars.insert("pr", &pr);
    vars.insert("branch", item.branch.as_deref().unwrap_or(""));
    vars.insert("context", item.context.as_deref().unwrap_or(""));
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);

    settings::config::render_prompt("advisor", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Check if the given intent is allowed from the task's current status.
use super::routes_task_advisor_helpers::{action_eligible, clear_advisor_session};
