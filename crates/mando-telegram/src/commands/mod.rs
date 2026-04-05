//! Command handlers — one module per Telegram command.

pub mod action;
mod action_sessions;
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
) -> anyhow::Result<Vec<mando_types::Task>> {
    load_tasks_with_path(gw, "/api/tasks").await
}

/// Load tasks from a specific API path (supports query params).
pub(crate) async fn load_tasks_with_path(
    gw: &crate::http::GatewayClient,
    path: &str,
) -> anyhow::Result<Vec<mando_types::Task>> {
    let resp = gw.get(path).await?;
    let items =
        serde_json::from_value::<Vec<mando_types::Task>>(resp["items"].clone()).map_err(|e| {
            tracing::error!(module = "commands", error = %e, "failed to deserialize task items");
            e
        })?;
    Ok(items)
}

/// Load tasks with user-visible error handling. Returns `None` (and sends an
/// error message to the chat) when the gateway call fails, preventing orphaned
/// loading placeholders.
pub(crate) async fn load_tasks_or_notify(
    bot: &crate::bot::TelegramBot,
    chat_id: &str,
) -> Option<Vec<mando_types::Task>> {
    match load_tasks(bot.gw()).await {
        Ok(items) => Some(items),
        Err(e) => {
            if let Err(e) = bot
                .send_html(
                    chat_id,
                    &format!(
                        "\u{274c} Failed to load tasks: {}",
                        mando_shared::telegram_format::escape_html(&e.to_string())
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
    let id = mando_uuid::Uuid::v4().to_string();
    id.replace('-', "")[..8].to_string()
}
