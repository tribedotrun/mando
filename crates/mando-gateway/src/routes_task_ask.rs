//! Task ask route handlers — multi-turn Q&A sessions with worktree access.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct AskBody {
    pub id: i64,
    pub question: String,
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
        let item = store.find_by_id(id).await.unwrap_or(None).ok_or_else(|| {
            error_response(StatusCode::NOT_FOUND, &format!("item {id} not found"))
        })?;
        (item, store.pool().clone())
    };

    // Resolve worktree cwd — fall back to first project path if no worktree.
    let cwd = item
        .worktree
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
        })?;

    let session_key = format!("task-ask:{id}");
    let mut mgr = state.cc_session_mgr.write().await;

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
        if let Ok(Some(mut task)) = store.find_by_id(id).await {
            task.session_ids.ask = None;
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(task_id = id, error = %e, "failed to clear stale session_ids.ask");
            }
        }
    }

    let result = if should_resume {
        // Follow-up: resume existing session with just the question.
        mgr.follow_up(&session_key, &body.question, &cwd)
            .await
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    } else {
        // First ask: build initial prompt with full task context.
        let task_id_str = id.to_string();
        let timeline_text = mando_captain::runtime::task_ask::build_timeline_text(&pool, id).await;
        let prompt = mando_captain::runtime::task_ask::build_initial_prompt(
            &item,
            &task_id_str,
            &body.question,
            &workflow,
            &timeline_text,
        )
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

        mgr.start_with_item(
            &session_key,
            &prompt,
            &cwd,
            Some(&workflow.models.captain),
            std::time::Duration::from_secs(3600),
            std::time::Duration::from_secs(120),
            &task_id_str,
        )
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    };

    // Drop manager lock before task store operations.
    drop(mgr);

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

    // Record Q&A history + timeline event.
    mando_captain::runtime::task_ask::record_ask(&pool, id, &body.question, &answer).await;

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "ask", "id": id})),
    );

    Ok(Json(json!({
        "id": id,
        "question": body.question,
        "answer": answer,
        "session_id": session_id,
    })))
}

/// POST /api/tasks/ask/end — end the ask session for a task.
pub(crate) async fn post_task_ask_end(
    State(state): State<AppState>,
    Json(body): Json<AskEndBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let session_key = format!("task-ask:{id}");

    let mut mgr = state.cc_session_mgr.write().await;
    mgr.close(&session_key);
    drop(mgr);

    // Clear session_ids.ask on the task.
    let store = state.task_store.write().await;
    if let Ok(Some(mut task)) = store.find_by_id(id).await {
        task.session_ids.ask = None;
        if let Err(e) = store.write_task(&task).await {
            tracing::warn!(task_id = id, error = %e, "failed to clear session_ids.ask on end");
        }
    }

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "ask_end", "id": id})),
    );

    Ok(Json(json!({"ok": true, "ended": session_key})))
}

/// Close ask session for a task (used by reopen/rework handlers).
pub(crate) async fn close_ask_session(state: &AppState, task_id: i64) {
    let ask_key = format!("task-ask:{task_id}");
    let mut mgr = state.cc_session_mgr.write().await;
    mgr.close(&ask_key);
}
