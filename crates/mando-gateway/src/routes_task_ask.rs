//! Task ask route handlers — multi-turn Q&A sessions with worktree access.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct AskBody {
    pub id: i64,
    pub question: String,
    /// Continue an existing conversation. None = start new Q&A session.
    #[serde(default)]
    pub ask_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct AskEndBody {
    pub id: i64,
}

/// POST /api/tasks/ask — multi-turn ask with worktree access.
///
/// First ask creates a new CC session in the task's worktree.
/// Follow-up asks resume the same session via `--resume`.
pub(crate) async fn post_task_ask(
    State(state): State<AppState>,
    Json(body): Json<AskBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let workflow = state.captain_workflow.load_full();

    // Load task + pool.
    let (item, pool) = {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {id} not found"))
            })?;
        (item, store.pool().clone())
    };

    let cwd = resolve_ask_cwd(&item, &state)?;

    let session_key = format!("task-ask:{id}");
    let mgr = state.cc_session_mgr.clone();

    let mgr_has_session = mgr.has_session(&session_key);
    let task_has_session = item.session_ids.ask.is_some();

    // Only resume if BOTH the manager has the session AND the task still
    // references it. If session_ids.ask was cleared (reopen/rework/revert),
    // close the stale in-memory session and start fresh.
    let should_resume = mgr_has_session && task_has_session;

    if mgr_has_session && !task_has_session {
        tracing::info!(
            task_id = id,
            "session_ids.ask cleared by lifecycle transition — closing stale session"
        );
        mgr.close(&session_key);
    } else if !mgr_has_session && task_has_session {
        tracing::warn!(
            task_id = id,
            "stale session_ids.ask — manager has no session, clearing"
        );
        let store = state.task_store.write().await;
        match store.find_by_id(id).await {
            Ok(Some(mut task)) => {
                task.session_ids.ask = None;
                if let Err(e) = store.write_task(&task).await {
                    tracing::warn!(
                        task_id = id,
                        error = %e,
                        "failed to clear stale session_ids.ask"
                    );
                }
            }
            Ok(None) => {
                tracing::warn!(
                    task_id = id,
                    "stale session_ids.ask clear skipped — task vanished between lookups"
                );
            }
            Err(e) => {
                tracing::warn!(
                    task_id = id,
                    error = %e,
                    "stale session_ids.ask clear skipped — task store read failed"
                );
            }
        }
    }

    // Generate or reuse ask_id (conversation grouping key).
    let ask_id = body
        .ask_id
        .clone()
        .unwrap_or_else(|| mando_uuid::Uuid::v4().to_string());

    // The real CC session_id isn't available until start_with_item returns.
    // Use a clear sentinel so question rows are distinguishable from answer rows.
    const PENDING_SESSION: &str = "pending";

    // ── Persist question immediately (before CC call) ───────────────────
    if let Err(e) = mando_captain::runtime::task_ask::persist_question(
        &pool,
        id,
        &ask_id,
        PENDING_SESSION,
        &body.question,
    )
    .await
    {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        ));
    }

    // Broadcast so the UI shows the question immediately.
    broadcast_task_update(&state, id).await;

    // ── Run the CC session ──────────────────────────────────────────────
    let cc_result = if should_resume {
        mgr.follow_up(&session_key, &body.question, &cwd).await
    } else {
        let task_id_str = id.to_string();
        let timeline_text = mando_captain::runtime::task_ask::build_timeline_text(&pool, id)
            .await
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
        let prompt = mando_captain::runtime::task_ask::build_initial_prompt(
            &item,
            &task_id_str,
            &body.question,
            &workflow,
            &timeline_text,
        )
        .map_err(crate::response::internal_error)?;

        mgr.start_with_item(
            &session_key,
            &prompt,
            &cwd,
            Some(&workflow.models.captain),
            workflow.agent.task_ask_idle_ttl_s,
            workflow.agent.task_ask_timeout_s,
            Some(id),
            None,
        )
        .await
    };

    drop(mgr);

    // ── Handle CC result ────────────────────────────────────────────────
    match cc_result {
        Ok(result) => {
            let answer = result.text.clone();
            let session_id = result.session_id.clone();

            // Persist session_ids.ask on the task if this is a new session.
            if !should_resume {
                let store = state.task_store.write().await;
                if let Ok(Some(mut task)) = store.find_by_id(id).await {
                    task.session_ids.ask = Some(session_id.clone());
                    if let Err(e) = store.write_task(&task).await {
                        tracing::warn!(task_id = id, error = %e, "failed to persist session_ids.ask");
                    }
                }
            }

            // Persist the answer + timeline event. Propagate as 500 so the
            // caller sees the inconsistency instead of receiving an answer
            // that was never written to history.
            mando_captain::runtime::task_ask::persist_answer(
                &pool,
                id,
                &ask_id,
                &session_id,
                &body.question,
                &answer,
            )
            .await
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

            broadcast_task_update(&state, id).await;

            Ok(Json(json!({
                "id": id,
                "ask_id": ask_id,
                "question": body.question,
                "answer": answer,
                "session_id": session_id,
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!(task_id = id, error = %error_msg, "ask CC session failed");

            // Close the broken session so the next ask starts fresh instead
            // of retrying the dead session indefinitely.
            state.cc_session_mgr.close(&session_key);
            {
                let store = state.task_store.write().await;
                if let Ok(Some(mut task)) = store.find_by_id(id).await {
                    if task.session_ids.ask.is_some() {
                        task.session_ids.ask = None;
                        let _ = store.write_task(&task).await;
                    }
                }
            }

            // Persist the error so it shows in the Q&A tab.
            if let Err(persist_err) = mando_captain::runtime::task_ask::persist_error(
                &pool,
                id,
                &ask_id,
                PENDING_SESSION,
                &body.question,
                &error_msg,
            )
            .await
            {
                tracing::error!(task_id = id, error = %persist_err, "failed to persist ask error");
            }

            broadcast_task_update(&state, id).await;

            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &error_msg,
            ))
        }
    }
}

/// POST /api/tasks/ask/end — end the ask session for a task.
pub(crate) async fn post_task_ask_end(
    State(state): State<AppState>,
    Json(body): Json<AskEndBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let session_key = format!("task-ask:{id}");

    state.cc_session_mgr.close(&session_key);

    // Clear session_ids.ask on the task.
    let store = state.task_store.write().await;
    match store.find_by_id(id).await {
        Ok(Some(mut task)) => {
            task.session_ids.ask = None;
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(task_id = id, error = %e, "failed to clear session_ids.ask on end");
            }
        }
        Ok(None) => {
            tracing::warn!(task_id = id, "ask_end clear skipped — task vanished");
        }
        Err(e) => return Err(internal_error(e)),
    }

    let updated = store
        .find_by_id(id)
        .await
        .ok()
        .flatten()
        .map(|t| serde_json::to_value(&t).unwrap());
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );

    Ok(Json(json!({"ok": true, "ended": session_key})))
}

/// POST /api/tasks/ask/reopen — synthesize Q&A into feedback and reopen.
///
/// Sends a follow-up to the active Q&A session asking it to produce a reopen
/// message, then closes the session and delegates to `action_contract::reopen_item`.
pub(crate) async fn post_task_ask_reopen(
    State(state): State<AppState>,
    Json(body): Json<AskEndBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    use mando_types::ItemStatus;

    let id = body.id;
    let workflow = state.captain_workflow.load_full();

    // ── Load task and guard status ───────────────────────────────────────
    let (item, pool) = {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {id} not found"))
            })?;
        (item, store.pool().clone())
    };

    // Only awaiting-review and escalated support both Q&A and reopen.
    let can_ask_reopen = matches!(
        item.status,
        ItemStatus::AwaitingReview | ItemStatus::Escalated
    );
    if !can_ask_reopen {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("cannot reopen from Q&A in status {:?}", item.status),
        ));
    }

    // ── Guard: Q&A history must be non-empty ─────────────────────────────
    let history = mando_db::queries::ask_history::load(&pool, id)
        .await
        .map_err(internal_error)?;
    if history.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "no Q&A history to synthesize — ask at least one question first",
        ));
    }

    let cwd = resolve_ask_cwd(&item, &state)?;

    // ── Synthesize via follow-up to active Q&A session ───────────────────
    let session_key = format!("task-ask:{id}");
    let mgr = &state.cc_session_mgr;

    if item.session_ids.ask.is_none() || !mgr.has_session(&session_key) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "no active Q&A session — use the standard reopen action instead",
        ));
    }

    let synthesis_prompt = workflow
        .prompts
        .get("task_ask_reopen_synthesis")
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_ask_reopen_synthesis prompt missing from captain-workflow.yaml",
            )
        })?
        .clone();

    let result = mgr
        .follow_up(&session_key, &synthesis_prompt, &cwd)
        .await
        .map_err(internal_error)?;
    let synthesized_feedback = result.text.clone();

    // ── Reopen the task with synthesized feedback ────────────────────────
    // Reopen BEFORE closing the ask session so the user keeps their Q&A
    // context if reopen fails.
    let config = state.config.load_full();
    let notifier = crate::captain_notifier(&state, &config);
    let store = state.task_store.write().await;
    let mut item = store
        .find_by_id(id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished during synthesis"))?;

    let old_session_id = item.session_ids.worker.clone();
    let outcome = mando_captain::runtime::action_contract::reopen_item(
        &mut item,
        "human",
        &synthesized_feedback,
        &config,
        &workflow,
        &notifier,
        store.pool(),
        true,
    )
    .await
    .map_err(internal_error)?;
    store.write_task(&item).await.map_err(internal_error)?;

    // Close the ask session only after reopen succeeds.
    close_ask_session(&state, id).await;

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": serde_json::to_value(&item).unwrap(), "id": id})),
    );

    // ── Timeline events ──────────────────────────────────────────────────
    let summary = format!("Reopened from Q&A: {}", &synthesized_feedback);
    let _ = mando_captain::runtime::timeline_emit::emit_for_task(
        &item,
        mando_types::timeline::TimelineEventType::HumanReopen,
        &summary,
        json!({
            "content": &synthesized_feedback,
            "source": "ask-reopen",
            "worker": item.worker,
            "session_id": item.session_ids.worker,
        }),
        store.pool(),
    )
    .await;

    if matches!(
        outcome,
        mando_captain::runtime::action_contract::ReopenOutcome::Reopened
    ) {
        let truly_resumed = old_session_id.is_some() && old_session_id == item.session_ids.worker;
        let (evt, evt_summary) = if truly_resumed {
            (
                mando_types::timeline::TimelineEventType::SessionResumed,
                format!("Resumed {}", item.worker.as_deref().unwrap_or("worker")),
            )
        } else {
            (
                mando_types::timeline::TimelineEventType::WorkerSpawned,
                format!("Spawned {}", item.worker.as_deref().unwrap_or("worker")),
            )
        };
        let _ = mando_captain::runtime::timeline_emit::emit_for_task(
            &item,
            evt,
            &evt_summary,
            json!({
                "worker": item.worker,
                "session_id": item.session_ids.worker,
            }),
            store.pool(),
        )
        .await;

        let msg = format!(
            "\u{1f504} Reopened <b>{}</b> from Q&A",
            mando_shared::telegram_format::escape_html(&item.title),
        );
        notifier.normal(&msg).await;
    }

    Ok(Json(json!({
        "ok": true,
        "feedback": synthesized_feedback,
    })))
}

/// Resolve the working directory for a task's ask session.
fn resolve_ask_cwd(
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
                "no worktree or project configured — cannot run ask session",
            )
        })
}

/// Broadcast a task update via SSE so the frontend refreshes.
async fn broadcast_task_update(state: &AppState, id: i64) {
    let updated = {
        let store = state.task_store.read().await;
        store
            .find_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|t| serde_json::to_value(&t).unwrap())
    };
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
}

/// Close ask session for a task (used by reopen/rework handlers).
pub(crate) async fn close_ask_session(state: &AppState, task_id: i64) {
    let ask_key = format!("task-ask:{task_id}");
    state.cc_session_mgr.close(&ask_key);
}
