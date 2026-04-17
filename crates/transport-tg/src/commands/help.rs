//! `/start` and `/help` command handlers.

use crate::bot::TelegramBot;
use crate::bot_helpers::dm_reply_keyboard;
use anyhow::Result;

/// Help text for DM chats.
const HELP: &str = "\u{1f99e} <b>Tasks</b>\n\
/todo [items] \u{2014} Add tasks\n\
/tasks [all] \u{2014} Show task list\n\
/action \u{2014} Pick a task and act on it\n\
/action cancel \u{2014} Close active session\n\n\
\u{1f3db}\u{fe0f} <b>System</b>\n\
/triage \u{2014} Rank pending-review PRs\n\
/health \u{2014} System health + active workers\n\
/stop \u{2014} Stop all active workers\n\
/timeline &lt;id&gt; \u{2014} Task timeline + Q&A history";

const SCOUT_HELP: &str = "\n\n\u{1f50d} <b>Scout</b>\n\
/scout_add &lt;url&gt; \u{2014} Add URL to Scout\n\
/scout_research &lt;topic&gt; \u{2014} AI-powered link discovery\n\
/scout_list [status] \u{2014} List scout items\n\
/scout_saved \u{2014} View saved items\n\
/scout \u{2014} Review processed items (swipe)";

/// Handle `/start` or `/help`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let scout_enabled = bot.config().read().await.features.scout;
    let keyboard = dm_reply_keyboard(scout_enabled);
    let text = if scout_enabled {
        format!("{HELP}{SCOUT_HELP}")
    } else {
        HELP.to_string()
    };
    bot.api()
        .send_message(chat_id, &text, Some("HTML"), Some(keyboard), true)
        .await?;
    Ok(())
}
