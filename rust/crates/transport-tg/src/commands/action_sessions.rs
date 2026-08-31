//! Session text handlers for `/action` input clarification.
//!
//! Extracted from action.rs for file length.

use crate::telegram_format::escape_html;
use anyhow::Result;
use api_types::ItemStatus;
use tracing::{info, warn};

use crate::bot::TelegramBot;

use super::action::status_short;

const RECLARIFYING_FOLLOW_UP_NOTE: &str =
    "We'll send the next question here when ready. Tap Answer or use /action to reply.";

// ── Input text handler (multi-turn clarification) ───────────────────

/// Handle plain-text messages for active input session. Returns `true` if consumed.
pub async fn handle_input_text(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    let session = match bot.input_session(chat_id).await {
        Some(session) => session,
        None => return Ok(false),
    };
    let task_id = session.task_id;
    let item_title = session.title;
    let _context_append_guard = bot.lock_context_append(task_id).await;

    let tasks_resp = bot
        .gw()
        .get_tasks(&api_types::TaskListQuery {
            include_archived: None,
        })
        .await?;

    let item = match tasks_resp
        .items
        .into_iter()
        .find(|candidate| candidate.id == task_id)
    {
        Some(item) => item,
        None => {
            bot.close_input_session(chat_id).await;
            bot.send_html(chat_id, "\u{26a0}\u{fe0f} Task no longer exists.")
                .await?;
            return Ok(true);
        }
    };

    let existing_context = item.context.clone();
    match item.status {
        ItemStatus::New
        | ItemStatus::Clarifying
        | ItemStatus::NeedsClarification
        | ItemStatus::Queued => {}
        _ => {
            bot.close_input_session(chat_id).await;
            bot.send_html(
                chat_id,
                &format!(
                    "\u{2139}\u{fe0f} Task is now {}. Use /action to pick again.",
                    status_short(item.status)
                ),
            )
            .await?;
            return Ok(true);
        }
    }

    let item_id = item.id;
    let ack = bot
        .send_html(chat_id, "\u{1f9ed} Clarifying\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    if item.status == ItemStatus::NeedsClarification {
        match bot
            .gw()
            .post_tasks_by_id_clarify(
                &api_types::TaskIdParams { id: item_id },
                &api_types::ClarifyQuery { wait: Some(false) },
                &api_types::ClarifyRequest {
                    answers: None,
                    answer: Some(text.to_string()),
                },
            )
            .await
        {
            Ok(_) => {
                // Async ack: the daemon committed the answer and spawned the
                // follow-up CC call on its task tracker. The next question (or
                // ready/escalate state) arrives via the existing TG notify
                // path, so close this session. The user re-engages via the
                // notification's Answer button or `/action` when the next
                // message lands.
                bot.close_input_session(chat_id).await;
                global_infra::best_effort!(
                    bot.edit_message(
                        chat_id,
                        ack_mid,
                        &format!(
                            "\u{1f9ed} Got your answer for <b>{}</b>.\n\n{}",
                            escape_html(&item_title),
                            RECLARIFYING_FOLLOW_UP_NOTE,
                        ),
                    )
                    .await,
                    "action_sessions: ack edit after async clarify"
                );
            }
            Err(e) => {
                info!("[input] clarify failed for '{}': {}", item_title, e);
                append_context_fallback(
                    bot,
                    chat_id,
                    ack_mid,
                    &item_title,
                    item_id,
                    existing_context.as_deref(),
                    text,
                )
                .await;
            }
        }
    } else {
        append_context_fallback(
            bot,
            chat_id,
            ack_mid,
            &item_title,
            item_id,
            existing_context.as_deref(),
            text,
        )
        .await;
    }

    Ok(true)
}

async fn append_context_fallback(
    bot: &TelegramBot,
    chat_id: &str,
    mid: i64,
    title: &str,
    item_id: i64,
    existing_context: Option<&str>,
    text: &str,
) {
    let existing = existing_context.unwrap_or_default().trim();
    let appended = if existing.is_empty() {
        format!("Human note: {text}")
    } else {
        format!("{existing}\n\nHuman note: {text}")
    };

    match bot
        .gw()
        .patch_tasks_by_id(
            &api_types::TaskIdParams { id: item_id },
            &api_types::TaskPatchRequest {
                context: Some(appended),
                original_prompt: None,
                is_bug_fix: None,
            },
        )
        .await
    {
        Ok(_) => {
            info!("Input: appended context for '{}'", title);
            global_infra::best_effort!(
                bot.edit_message(
                    chat_id,
                    mid,
                    &format!(
                        "\u{2705} Context appended to <b>{}</b>.",
                        escape_html(title)
                    ),
                )
                .await,
                "action_sessions: bot .edit_message( chat_id, mid, &format!( '\u{2705} Context"
            );
        }
        Err(e) => {
            warn!("Input: context append failed for '{}': {e}", title);
            global_infra::best_effort!(
                bot.edit_message(chat_id, mid, "\u{274c} Failed to append context.")
                    .await,
                "action_sessions: bot .edit_message(chat_id, mid, '\u{274c} Failed to append c"
            );
        }
    }
    bot.close_input_session(chat_id).await;
}

// ── Clarifier question fetch (for input sessions) ───────────────────

/// Fetch the latest clarifier questions for a task from the timeline and
/// format them for display in Telegram. Filters out self-answered entries
/// to mirror the Electron renderer's behavior.
pub(crate) async fn fetch_clarifier_questions(bot: &TelegramBot, item_id: &str) -> Option<String> {
    let id = item_id.parse().ok()?;
    let timeline = bot
        .gw()
        .get_tasks_by_id_timeline(&api_types::TaskIdParams { id })
        .await
        .ok()?;
    let questions = timeline.events.iter().rev().find_map(|e| match &e.data {
        api_types::TimelineEventPayload::ClarifyQuestion { questions: qs, .. }
            if !qs.is_empty() =>
        {
            Some(qs)
        }
        _ => None,
    })?;
    let lines: Vec<String> = questions
        .iter()
        .filter(|q| !q.self_answered)
        .enumerate()
        .map(|(i, q)| format!("{}. {}", i + 1, q.question))
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::extract::{Path, State};
    use axum::routing::{get, patch, post};
    use axum::{Json, Router};
    use serde_json::json;
    use serde_json::Value;
    use tokio::sync::{Mutex as AsyncMutex, RwLock};

    use super::*;
    use crate::http::GatewayClient;

    #[derive(Clone)]
    struct MockState {
        tasks: Arc<AsyncMutex<api_types::TaskListResponse>>,
        task_gets: Arc<AtomicUsize>,
        patches: Arc<Mutex<Vec<(i64, Value)>>>,
        list_delay: Duration,
    }

    fn task(id: i64, context: &str) -> api_types::TaskItem {
        api_types::TaskItem {
            id,
            rev: 1,
            title: "Duplicate title".to_string(),
            provider: api_types::TaskProvider::Claude,
            use_glm_worker: false,
            status: api_types::ItemStatus::Queued,
            project: Some("mando".to_string()),
            github_repo: None,
            branch: None,
            pr_number: None,
            project_id: Some(1),
            worker: None,
            session_ids: Some(api_types::SessionIds::default()),
            intervention_count: 0,
            captain_review_trigger: None,
            escalation_report: None,
            context: Some(context.to_string()),
            original_prompt: None,
            workbench_id: 1,
            worktree: None,
            plan: None,
            no_pr: false,
            no_auto_merge: false,
            is_bug_fix: false,
            resource: None,
            images: None,
            created_at: None,
            last_activity_at: None,
            worker_started_at: None,
            worker_seq: 0,
            reopen_seq: 0,
            reopened_at: None,
            reopen_source: None,
            review_fail_count: 0,
            clarifier_fail_count: 0,
            spawn_fail_count: 0,
            merge_fail_count: 0,
            source: None,
            paused_until: None,
        }
    }

    async fn list_tasks(State(state): State<MockState>) -> Json<api_types::TaskListResponse> {
        state.task_gets.fetch_add(1, Ordering::SeqCst);
        let snapshot = state.tasks.lock().await.clone();
        tokio::time::sleep(state.list_delay).await;
        Json(snapshot)
    }

    async fn patch_task(
        State(state): State<MockState>,
        Path(id): Path<i64>,
        Json(body): Json<Value>,
    ) -> Json<api_types::BoolOkResponse> {
        if let Some(context) = body.get("context").and_then(Value::as_str) {
            let mut tasks = state.tasks.lock().await;
            if let Some(task) = tasks.items.iter_mut().find(|task| task.id == id) {
                task.context = Some(context.to_string());
            }
        }
        state
            .patches
            .lock()
            .expect("patch capture lock")
            .push((id, body));
        Json(api_types::BoolOkResponse { ok: true })
    }

    async fn telegram_ok() -> Json<Value> {
        Json(json!({"ok": true, "result": {"message_id": 91}}))
    }

    #[tokio::test]
    async fn input_follow_up_uses_session_task_id_and_fetched_context() {
        let task_gets = Arc::new(AtomicUsize::new(0));
        let tasks = Arc::new(AsyncMutex::new(api_types::TaskListResponse {
            items: vec![task(41, "wrong context"), task(42, "selected context")],
            count: 2,
        }));
        let captured_patches = Arc::new(Mutex::new(Vec::new()));
        let state = MockState {
            tasks,
            task_gets: task_gets.clone(),
            patches: captured_patches.clone(),
            list_delay: Duration::ZERO,
        };
        let app = Router::new()
            .route(gateway_client::routes::GET_TASKS.path, get(list_tasks))
            .route(concat!("/api", "/tasks/{id}"), patch(patch_task))
            .route("/botTEST/sendMessage", post(telegram_ok))
            .route("/botTEST/editMessageText", post(telegram_ok))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("mock server address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock routes");
        });

        let base_url = format!("http://{addr}");
        let config = Arc::new(RwLock::new(settings::Config::default()));
        let pending = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let bot = TelegramBot::with_base_url(
            config,
            "TEST",
            Some(&base_url),
            GatewayClient::new(addr.port(), None),
            pending,
        )
        .expect("construct test bot");
        bot.open_input_session("chat", 42, "Duplicate title", 90)
            .await;

        assert!(handle_input_text(&bot, "chat", "new detail")
            .await
            .expect("handle input"));

        assert_eq!(task_gets.load(Ordering::SeqCst), 1);
        let captured = captured_patches.lock().expect("patch capture lock").clone();
        assert_eq!(captured.len(), 1);
        let (patched_id, body) = &captured[0];
        assert_eq!(*patched_id, 42);
        assert_eq!(
            body["context"],
            "selected context\n\nHuman note: new detail"
        );
        assert!(!bot.has_input_session("chat").await);

        server.abort();
    }

    #[tokio::test]
    async fn concurrent_input_replies_preserve_both_context_notes() {
        let task_gets = Arc::new(AtomicUsize::new(0));
        let tasks = Arc::new(AsyncMutex::new(api_types::TaskListResponse {
            items: vec![task(42, "original context")],
            count: 1,
        }));
        let captured_patches = Arc::new(Mutex::new(Vec::new()));
        let state = MockState {
            tasks: tasks.clone(),
            task_gets: task_gets.clone(),
            patches: captured_patches.clone(),
            list_delay: Duration::from_millis(75),
        };
        let app = Router::new()
            .route(gateway_client::routes::GET_TASKS.path, get(list_tasks))
            .route(concat!("/api", "/tasks/{id}"), patch(patch_task))
            .route("/botTEST/sendMessage", post(telegram_ok))
            .route("/botTEST/editMessageText", post(telegram_ok))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("mock server address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock routes");
        });

        let base_url = format!("http://{addr}");
        let config = Arc::new(RwLock::new(settings::Config::default()));
        let pending = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let bot = TelegramBot::with_base_url(
            config,
            "TEST",
            Some(&base_url),
            GatewayClient::new(addr.port(), None),
            pending,
        )
        .expect("construct test bot");
        bot.open_input_session("chat", 42, "Duplicate title", 90)
            .await;

        let (first, second) = tokio::join!(
            handle_input_text(&bot, "chat", "first detail"),
            handle_input_text(&bot, "chat", "second detail"),
        );
        assert!(first.expect("first input reply"));
        assert!(second.expect("second input reply"));

        assert_eq!(task_gets.load(Ordering::SeqCst), 2);
        assert_eq!(
            captured_patches.lock().expect("patch capture lock").len(),
            2
        );
        let final_context = tasks
            .lock()
            .await
            .items
            .first()
            .and_then(|task| task.context.clone())
            .expect("final context");
        assert!(final_context.starts_with("original context"));
        assert_eq!(final_context.matches("Human note: first detail").count(), 1);
        assert_eq!(
            final_context.matches("Human note: second detail").count(),
            1
        );

        server.abort();
    }
}
