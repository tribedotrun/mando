//! Command handlers — one module per Telegram command.

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

pub(crate) fn status_is_finalized(status: api_types::ItemStatus) -> bool {
    matches!(
        status,
        api_types::ItemStatus::Merged
            | api_types::ItemStatus::CompletedNoPr
            | api_types::ItemStatus::Canceled
    )
}

pub(crate) fn status_wire_name(status: api_types::ItemStatus) -> &'static str {
    match status {
        api_types::ItemStatus::New => "new",
        api_types::ItemStatus::Clarifying => "clarifying",
        api_types::ItemStatus::NeedsClarification => "needs-clarification",
        api_types::ItemStatus::Queued => "queued",
        api_types::ItemStatus::InProgress => "in-progress",
        api_types::ItemStatus::CaptainReviewing => "captain-reviewing",
        api_types::ItemStatus::CaptainMerging => "captain-merging",
        api_types::ItemStatus::AwaitingReview => "awaiting-review",
        api_types::ItemStatus::Rework => "rework",
        api_types::ItemStatus::HandedOff => "handed-off",
        api_types::ItemStatus::Escalated => "escalated",
        api_types::ItemStatus::Errored => "errored",
        api_types::ItemStatus::Merged => "merged",
        api_types::ItemStatus::CompletedNoPr => "completed-no-pr",
        api_types::ItemStatus::Canceled => "canceled",
        api_types::ItemStatus::Stopped => "stopped",
    }
}

/// Load and parse tasks via the gateway HTTP API.
pub(crate) async fn load_tasks(
    gw: &crate::http::GatewayClient,
) -> anyhow::Result<Vec<api_types::TaskItem>> {
    load_tasks_with_archived(gw, false).await
}

/// Load tasks with an explicit archived-items policy.
///
pub(crate) async fn load_tasks_with_archived(
    gw: &crate::http::GatewayClient,
    include_archived: bool,
) -> anyhow::Result<Vec<api_types::TaskItem>> {
    let resp = gw
        .get_tasks(&api_types::TaskListQuery {
            include_archived: include_archived.then_some(true),
        })
        .await?;
    Ok(resp.items)
}

/// Load tasks with user-visible error handling. Returns `None` (and sends an
/// error message to the chat) when the gateway call fails, preventing orphaned
/// loading placeholders.
pub(crate) async fn load_tasks_or_notify(
    bot: &crate::bot::TelegramBot,
    chat_id: &str,
) -> Option<Vec<api_types::TaskItem>> {
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
