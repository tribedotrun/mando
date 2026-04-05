//! `/start` and `/help` command handlers.

use crate::bot::TelegramBot;
use crate::bot_helpers::dm_reply_keyboard;
use anyhow::Result;

/// Help text for DM chats.
const HELP: &str = "\u{1f99e} <b>Tasks</b>\n\
/todo [items] \u{2014} Add tasks\n\
/tasks [all] \u{2014} Show task list\n\
/accept &lt;id&gt; \u{2014} Accept a no-PR task\n\
/reopen \u{2014} Reopen done/failed task with feedback\n\
/rework \u{2014} Rework a task with fresh worker\n\
/handoff \u{2014} Hand off a task to human\n\
/adopt &lt;path&gt; &lt;title&gt; [--project &lt;name&gt;] [--branch &lt;name&gt;] [--note &lt;text&gt;] \u{2014} Adopt human\u{2019}s worktree\n\
/input \u{2014} Clarify or add context\n\
/answer &lt;id&gt; &lt;text&gt; \u{2014} Answer task clarifier questions\n\
/retry &lt;id&gt; \u{2014} Retry an errored captain review\n\
/cancel [id] \u{2014} Cancel a task\n\
/delete [id] \u{2014} Permanently remove a task\n\n\
\u{1f3db}\u{fe0f} <b>Captain</b>\n\
/captain \u{2014} Run captain tick now\n\
/workers \u{2014} Show active workers\n\
/nudge &lt;id&gt; &lt;msg&gt; \u{2014} Nudge a stuck worker\n\
/stop \u{2014} Stop all active workers\n\
/triage \u{2014} Rank pending-review PRs\n\
/health \u{2014} System health (daemon, workers, config)\n\n\
\u{1f50d} <b>Scout</b>\n\
/scout_add &lt;url&gt; \u{2014} Add URL to Scout\n\
/scout_research &lt;topic&gt; \u{2014} AI-powered link discovery\n\
/scout_publish &lt;id&gt; \u{2014} Publish extracted Scout article\n\
/scout_list [status] \u{2014} List scout items with summaries\n\
/scout_simple [status] \u{2014} List scout items (compact)\n\
/scout_saved \u{2014} View saved items\n\
/scout \u{2014} Review processed items (swipe)\n\n\
\u{1f4ac} <b>Interactive</b>\n\
/ask \u{2014} Q&A on completed tasks\n\
/timeline [id] [chat] \u{2014} Lifecycle timeline\n\
/prsummary &lt;id&gt; \u{2014} Show PR description\n\
/history [id] \u{2014} Ask history for a task\n\
/sessions \u{2014} Recent CC sessions";

/// Handle `/start` or `/help`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    bot.api()
        .send_message(chat_id, HELP, Some("HTML"), Some(dm_reply_keyboard()), true)
        .await?;
    Ok(())
}
