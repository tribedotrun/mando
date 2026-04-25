//! Command handlers — one module per Telegram command.

use crate::gateway_paths as paths;

pub mod action;
mod action_sessions;
pub mod detail;
pub mod health;
pub mod help;
pub mod status;
pub mod stop;
pub mod timeline;
pub mod todo;
pub mod triage;

// ── Shared helpers ───────────────────────────────────────────────────

/// Load and parse tasks via the gateway HTTP API.
pub(crate) async fn load_tasks(
    gw: &crate::http::GatewayClient,
) -> anyhow::Result<Vec<captain::Task>> {
    load_tasks_with_path(gw, paths::TASKS).await
}

/// Load tasks from a specific API path (supports query params).
///
/// Fail-fast: a wire-type conversion failure on any task is treated as
/// an infrastructure error and propagated. Previously `filter_map +
/// .ok()` silently omitted the offending task from the list, so users
/// saw a shorter task list with no indication anything was wrong.
pub(crate) async fn load_tasks_with_path(
    gw: &crate::http::GatewayClient,
    path: &str,
) -> anyhow::Result<Vec<captain::Task>> {
    let resp = gw.get_typed::<api_types::TaskListResponse>(path).await?;
    resp.items
        .into_iter()
        .map(|item| {
            let task_id = item.id;
            let value = serde_json::to_value(&item).map_err(|e| {
                anyhow::anyhow!("failed to serialize TaskItem {task_id} for TG command: {e}")
            })?;
            serde_json::from_value::<captain::Task>(value).map_err(|e| {
                anyhow::anyhow!(
                    "failed to convert TaskItem {task_id} to Task (api-types schema drift): {e}"
                )
            })
        })
        .collect()
}

/// Load tasks with user-visible error handling. Returns `None` (and sends an
/// error message to the chat) when the gateway call fails, preventing orphaned
/// loading placeholders.
pub(crate) async fn load_tasks_or_notify(
    bot: &crate::bot::TelegramBot,
    chat_id: &str,
) -> Option<Vec<captain::Task>> {
    match load_tasks(bot.gw()).await {
        Ok(items) => Some(items),
        Err(e) => {
            if let Err(e) = bot
                .send_html(
                    chat_id,
                    &format!(
                        "\u{274c} Failed to load tasks: {}",
                        crate::telegram_format::escape_html(&e.to_string())
                    ),
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            None
        }
    }
}

/// Truncate a string at a UTF-8 char boundary.
pub(crate) fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
}

/// Generate a short (8 hex char) unique ID for action tracking.
pub(crate) fn short_uuid() -> String {
    let id = global_infra::uuid::Uuid::v4().to_string();
    id.replace('-', "")[..8].to_string()
}
