//! Task advisor route handlers -- persistent per-task advisor sessions.
//!
//! The advisor is a lazy-spawned CC session that serves as the user's
//! interface to a task. It can answer questions, synthesize reopen/rework
//! requests, and dispatch actions to captain.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

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
            .map_err(internal_error)?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
            })?;
        (item, store.pool().clone())
    };

    let cwd = resolve_advisor_cwd(&item, &state)?;

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
        let store = state.task_store.write().await;
        if let Ok(Some(mut task)) = store.find_by_id(task_id).await {
            task.session_ids.advisor = None;
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(task_id, error = %e, "failed to clear stale session_ids.advisor");
            }
        }
    }

    // Generate ask_id for conversation tracking.
    let ask_id = mando_uuid::Uuid::v4().to_string();

    // Persist user message immediately.
    const PENDING_SESSION: &str = "pending";
    if let Err(e) = mando_captain::runtime::task_ask::persist_question(
        &pool,
        task_id,
        &ask_id,
        PENDING_SESSION,
        &body.message,
    )
    .await
    {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        ));
    }

    broadcast_task_update(&state, task_id).await;

    // Run the CC session.
    let cc_result = if should_resume {
        mgr.follow_up(&session_key, &body.message, &cwd).await
    } else {
        let task_id_str = task_id.to_string();
        let timeline_text = mando_captain::runtime::task_ask::build_timeline_text(&pool, task_id)
            .await
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

        let prompt = build_advisor_prompt(
            &item,
            &task_id_str,
            &body.message,
            &workflow,
            &timeline_text,
        )
        .map_err(internal_error)?;

        mgr.start_with_item(
            &session_key,
            &prompt,
            &cwd,
            Some(&workflow.models.captain),
            workflow.agent.task_ask_idle_ttl_s,
            workflow.agent.task_ask_timeout_s,
            Some(task_id),
            None,
        )
        .await
    };

    drop(mgr);

    match cc_result {
        Ok(result) => {
            let answer = result.text.clone();
            let session_id = result.session_id.clone();

            // Persist session_ids.advisor on the task.
            if !should_resume {
                let store = state.task_store.write().await;
                if let Ok(Some(mut task)) = store.find_by_id(task_id).await {
                    task.session_ids.advisor = Some(session_id.clone());
                    if let Err(e) = store.write_task(&task).await {
                        tracing::warn!(task_id, error = %e, "failed to persist session_ids.advisor");
                    }
                }
            }

            // Persist answer + timeline event.
            mando_captain::runtime::task_ask::persist_answer(
                &pool,
                task_id,
                &ask_id,
                &session_id,
                &body.message,
                &answer,
                &body.intent,
            )
            .await
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

            broadcast_task_update(&state, task_id).await;

            // Handle reopen/rework intent.
            if body.intent == "reopen" || body.intent == "rework" {
                return handle_advisor_action(
                    &state,
                    task_id,
                    &session_key,
                    &body.intent,
                    &cwd,
                    &workflow,
                    &answer,
                )
                .await;
            }

            Ok(Json(json!({
                "id": task_id,
                "ask_id": ask_id,
                "message": body.message,
                "answer": answer,
                "session_id": session_id,
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!(task_id, error = %error_msg, "advisor CC session failed");

            // Close broken session.
            state.cc_session_mgr.close(&session_key);
            {
                let store = state.task_store.write().await;
                if let Ok(Some(mut task)) = store.find_by_id(task_id).await {
                    if task.session_ids.advisor.is_some() {
                        task.session_ids.advisor = None;
                        if let Err(e) = store.write_task(&task).await {
                            tracing::warn!(task_id, error = %e, "failed to clear session_ids.advisor on error");
                        }
                    }
                }
            }

            if let Err(persist_err) = mando_captain::runtime::task_ask::persist_error(
                &pool,
                task_id,
                &ask_id,
                PENDING_SESSION,
                &body.message,
                &error_msg,
            )
            .await
            {
                tracing::error!(task_id, error = %persist_err, "failed to persist advisor error");
            }

            broadcast_task_update(&state, task_id).await;

            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &error_msg,
            ))
        }
    }
}

/// Handle reopen/rework via advisor synthesis.
async fn handle_advisor_action(
    state: &AppState,
    task_id: i64,
    session_key: &str,
    intent: &str,
    cwd: &std::path::Path,
    workflow: &mando_config::CaptainWorkflow,
    _initial_answer: &str,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check status eligibility BEFORE the expensive CC synthesis call.
    {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(task_id)
            .await
            .map_err(internal_error)?
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
    let synthesis_prompt_key = "advisor_reopen_synthesis";
    let synthesis_prompt = workflow
        .prompts
        .get(synthesis_prompt_key)
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("{synthesis_prompt_key} prompt missing from captain-workflow.yaml"),
            )
        })?
        .clone();

    let result = mgr
        .follow_up(session_key, &synthesis_prompt, cwd)
        .await
        .map_err(internal_error)?;
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
        .map_err(internal_error)?
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

    if intent == "rework" {
        mando_captain::runtime::dashboard::rework_item(&store, task_id, &synthesized_feedback)
            .await
            .map_err(internal_error)?;
        // Reload after rework.
        item = store
            .find_by_id(task_id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished after rework"))?;
    } else {
        let _outcome = mando_captain::runtime::action_contract::reopen_item(
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
        .map_err(internal_error)?;
        store.write_task(&item).await.map_err(internal_error)?;
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
        mando_types::timeline::TimelineEventType::ReworkRequested
    } else {
        mando_types::timeline::TimelineEventType::HumanReopen
    };
    let summary = format!(
        "{} from advisor: {}",
        if intent == "rework" {
            "Rework"
        } else {
            "Reopened"
        },
        &synthesized_feedback,
    );
    if let Err(e) = mando_captain::runtime::timeline_emit::emit_for_task(
        &item,
        event_type,
        &summary,
        json!({
            "content": &synthesized_feedback,
            "source": "advisor",
        }),
        store.pool(),
    )
    .await
    {
        tracing::warn!(task_id, error = %e, "failed to emit advisor timeline event");
    }

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": serde_json::to_value(&item).unwrap(), "id": task_id})),
    );

    Ok(Json(json!({
        "ok": true,
        "intent": intent,
        "feedback": synthesized_feedback,
    })))
}

/// Build the initial advisor prompt from the workflow template.
fn build_advisor_prompt(
    item: &mando_types::Task,
    task_id: &str,
    question: &str,
    workflow: &mando_config::CaptainWorkflow,
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
    let branch = item.branch.as_deref().unwrap_or("");
    vars.insert("branch", branch);
    let context = item.context.as_deref().unwrap_or("");
    vars.insert("context", context);
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);

    mando_config::render_prompt("advisor", &workflow.prompts, &vars).map_err(|e| anyhow::anyhow!(e))
}

/// Resolve the working directory for an advisor session.
fn resolve_advisor_cwd(
    item: &mando_types::Task,
    state: &AppState,
) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    item.worktree
        .as_deref()
        .map(mando_config::expand_tilde)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let cfg = state.config.load_full();
            mando_config::paths::first_project_path(&cfg)
                .map(|p| mando_config::paths::expand_tilde(&p))
                .filter(|p| p.is_dir())
        })
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "no worktree or project configured -- cannot run advisor session",
            )
        })
}

/// Check if the given intent is allowed from the task's current status.
fn action_eligible(intent: &str, status: &mando_types::ItemStatus) -> bool {
    use mando_types::ItemStatus;
    if intent == "rework" {
        matches!(
            status,
            ItemStatus::AwaitingReview
                | ItemStatus::Escalated
                | ItemStatus::Errored
                | ItemStatus::HandedOff
        )
    } else {
        matches!(
            status,
            ItemStatus::AwaitingReview
                | ItemStatus::Escalated
                | ItemStatus::Errored
                | ItemStatus::HandedOff
                | ItemStatus::CompletedNoPr
        )
    }
}

/// Broadcast a task update via SSE.
async fn broadcast_task_update(state: &AppState, id: i64) {
    let updated = {
        let store = state.task_store.read().await;
        match store.find_by_id(id).await {
            Ok(Some(task)) => Some(serde_json::to_value(&task).unwrap()),
            Ok(None) => {
                tracing::warn!(task_id = id, "broadcast skipped -- task not found");
                return;
            }
            Err(e) => {
                tracing::warn!(task_id = id, error = %e, "broadcast skipped -- DB read failed");
                return;
            }
        }
    };
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
}
