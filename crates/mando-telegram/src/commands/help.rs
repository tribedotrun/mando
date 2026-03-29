//! `/start` and `/help` command handlers.

use crate::bot::TelegramBot;
use crate::bot_helpers::dm_reply_keyboard;
use anyhow::Result;

/// Help text for private (DM) chats.
const HELP_PRIVATE: &str = "\u{1f99e} <b>Tasks</b>\n\
/todo [items] \u{2014} Add backlog items\n\
/status [all] \u{2014} Show backlog status\n\
/reopen \u{2014} Reopen done/failed item with feedback\n\
/rework \u{2014} Rework done item with fresh worker\n\
/handoff \u{2014} Hand off item to human\n\
/adopt <path> <title> \u{2014} Adopt human\u{2019}s worktree\n\
/input \u{2014} Clarify or add context\n\
/answer <id> <text> \u{2014} Answer clarifier questions\n\
/retry <id> \u{2014} Retry errored captain review\n\
/cancel [id] \u{2014} Cancel a backlog item\n\
/delete [id] \u{2014} Permanently remove a backlog item\n\n\
\u{1f3db}\u{fe0f} <b>Captain</b>\n\
/captain \u{2014} Run captain tick now\n\
/cron [list|add|enable|disable|delete|test] \u{2014} Manage cron\n\
/sessions \u{2014} List recent CC sessions\n\
/knowledge \u{2014} Approve pending knowledge lessons\n\
/triage \u{2014} Rank pending-review PRs\n\
/health \u{2014} System health (daemon, workers, config)\n\n\
\u{1f50d} <b>Research</b>\n\
/addlink <url> \u{2014} Add URL to scout queue\n\
/research <topic> \u{2014} AI-powered link discovery\n\
/list [status] \u{2014} List scout items with summaries\n\
/simplelist [status] \u{2014} List scout items (compact)\n\
/saved \u{2014} View saved items\n\
/scout \u{2014} Review processed items (swipe)\n\n\
\u{1f4ac} <b>Interactive</b>\n\
/ops <msg> \u{2014} Ops copilot\n\
/ask \u{2014} Q&A on completed items\n\
/timeline [id] [chat] \u{2014} Lifecycle timeline\n\
/prsummary <id> \u{2014} Show PR description\n\
/history [id] \u{2014} Ask history for an item";

/// Help text for group chats (subset of commands).
const HELP_GROUP: &str = "\u{1f99e} mando commands:\n\
/status [all] \u{2014} Show backlog status\n\
/health \u{2014} System health (daemon, workers, config)\n\
/ask \u{2014} Q&A on completed items\n\
/addlink <url> \u{2014} Add URL to scout queue\n\
/research <topic> \u{2014} AI-powered link discovery\n\
/list [status] \u{2014} List scout items\n\
/scout \u{2014} Review processed items";

/// Handle `/start` or `/help`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str, is_group: bool) -> Result<()> {
    let text = if is_group { HELP_GROUP } else { HELP_PRIVATE };
    if is_group {
        bot.send_html(chat_id, text).await?;
    } else {
        // Send with persistent reply keyboard in DM
        bot.api()
            .send_message(chat_id, text, Some("HTML"), Some(dm_reply_keyboard()), true)
            .await?;
    }
    Ok(())
}
