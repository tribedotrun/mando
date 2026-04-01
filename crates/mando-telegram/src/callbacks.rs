//! Inline keyboard callback handlers.
//!
//! Dispatch by prefix: `{prefix}:{action}:{params}`.

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use crate::bot::TelegramBot;
use crate::callbacks_picker;
use crate::callbacks_session;

/// Handle an incoming callback query.
pub async fn handle_callback(bot: &mut TelegramBot, callback: &Value) -> Result<()> {
    let data = callback.get("data").and_then(|d| d.as_str()).unwrap_or("");
    let cb_id = callback.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let chat_id = callback
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default();
    let mid = callback
        .get("message")
        .and_then(|m| m.get("message_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    debug!("Callback: data={data} chat={chat_id}");
    let parts: Vec<&str> = data.split(':').collect();
    let prefix = parts.first().copied().unwrap_or("");

    match prefix {
        "todo_confirm" => handle_todo_confirm(bot, &parts, cb_id, &chat_id, mid).await,
        "todo_project" => handle_todo_project(bot, &parts, cb_id, &chat_id, mid).await,
        "swarm_input" => {
            callbacks_picker::handle_picker(bot, "input", &parts, cb_id, &chat_id, mid).await
        }
        "reopen" => {
            callbacks_picker::handle_picker(bot, "reopen", &parts, cb_id, &chat_id, mid).await
        }
        "rework" => {
            callbacks_picker::handle_picker(bot, "rework", &parts, cb_id, &chat_id, mid).await
        }
        "handoff" => {
            callbacks_picker::handle_picker(bot, "handoff", &parts, cb_id, &chat_id, mid).await
        }
        "merge" => handle_action(bot, "Merge", &parts, cb_id, &chat_id).await,
        "accept" => handle_action(bot, "Accept", &parts, cb_id, &chat_id).await,
        "answer" => handle_answer_cb(bot, &parts, cb_id, &chat_id).await,
        "retry" => handle_retry_cb(bot, &parts, cb_id, &chat_id).await,
        "view" => handle_view_cb(bot, &parts, cb_id, &chat_id).await,
        "ms_cancel" | "ms_delete" => {
            handle_multi_select(bot, prefix, &parts, cb_id, &chat_id, mid).await
        }
        "cron_act" => handle_cron_act(bot, &parts, cb_id, &chat_id, mid).await,
        "captain_learn" => handle_captain_learn(bot, cb_id, &chat_id).await,
        "knowledge" => {
            callbacks_session::handle_knowledge_callback(bot, &parts, cb_id, &chat_id, mid).await
        }
        "ops" => callbacks_session::handle_ops_callback(bot, &parts, cb_id, &chat_id, mid).await,
        "ask" => callbacks_session::handle_ask_callback(bot, &parts, cb_id, &chat_id, mid).await,
        "dg" => crate::assistant::callbacks::handle_callback(bot, callback).await,
        other => {
            tracing::warn!(action = other, "unrecognized callback prefix");
            bot.api().answer_callback_query(cb_id, None).await?;
            Ok(())
        }
    }
}

// ── Todo confirm ─────────────────────────────────────────────────────

async fn handle_todo_confirm(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    let aid = parts.get(2).copied().unwrap_or("");
    match action {
        "confirm" => {
            // Confirm is only shown when all items already have a project
            // (prefix match or single project). Multi-project uses todo_project directly.
            if let Some(state) = bot.take_todo_confirm(aid) {
                bot.api()
                    .answer_callback_query(cb_id, Some("Writing\u{2026}"))
                    .await?;
                if let Err(e) = bot
                    .edit_message(cid, mid, "\u{23f3} Writing to task list\u{2026}")
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
                crate::callback_actions::add_todo_items(bot, cid, &state.items).await?;
            } else {
                bot.api()
                    .answer_callback_query(cb_id, Some("Expired"))
                    .await?;
            }
        }
        "edit" => {
            bot.api()
                .answer_callback_query(cb_id, Some("Send your edits"))
                .await?;
            if let Err(e) = bot
                .edit_message(cid, mid, "\u{270f}\u{fe0f} Send your edit instructions:")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "cancel" => {
            bot.take_todo_confirm(aid);
            bot.api()
                .answer_callback_query(cb_id, Some("Cancelled"))
                .await?;
            if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Cancelled").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}

// ── Todo project picker ──────────────────────────────────────────────

async fn handle_todo_project(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let aid = parts.get(1).copied().unwrap_or("");
    let sel = parts.get(2).copied().unwrap_or("");
    if sel == "cancel" {
        bot.take_todo_confirm(aid);
        bot.api()
            .answer_callback_query(cb_id, Some("Cancelled"))
            .await?;
        if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Cancelled").await {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
        return Ok(());
    }
    if let Some(mut state) = bot.take_todo_confirm(aid) {
        // Resolve numeric index to project name.
        let idx: usize = sel.parse().unwrap_or(usize::MAX);
        let name = state.picker_slugs.get(idx).cloned().unwrap_or_default();
        if name.is_empty() {
            bot.api()
                .answer_callback_query(cb_id, Some("Invalid selection"))
                .await?;
            return Ok(());
        }
        // Assign the selected project to all items that don't have one yet.
        for item in &mut state.items {
            if item.project.is_none() {
                item.project = Some(name.clone());
            }
        }
        bot.api().answer_callback_query(cb_id, Some(&name)).await?;
        if let Err(e) = bot
            .edit_message(
                cid,
                mid,
                &format!(
                    "\u{23f3} Writing to <b>{}</b>\u{2026}",
                    mando_shared::escape_html(&name)
                ),
            )
            .await
        {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
        crate::callback_actions::add_todo_items(bot, cid, &state.items).await?;
    } else {
        bot.api()
            .answer_callback_query(cb_id, Some("Expired"))
            .await?;
    }
    Ok(())
}

// ── Merge / Accept ───────────────────────────────────────────────────

async fn handle_action(
    bot: &TelegramBot,
    label: &str,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some(&format!("{label}\u{2026}")))
        .await?;

    use crate::callback_actions;
    if label == "Merge" {
        callback_actions::merge(bot, cid, item_id).await?;
    } else {
        callback_actions::accept(bot, cid, item_id).await?;
    }
    Ok(())
}

// ── Multi-select ─────────────────────────────────────────────────────

async fn handle_multi_select(
    bot: &mut TelegramBot,
    prefix: &str,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    let aid = parts.get(2).copied().unwrap_or("");
    match action {
        "toggle" => {
            let idx: usize = parts
                .get(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(usize::MAX);
            if let Some(picker) = bot.ms_picker_mut(prefix, aid) {
                if picker.selected.contains(&idx) {
                    picker.selected.remove(&idx);
                } else if idx < picker.items.len() {
                    picker.selected.insert(idx);
                }
                let count = picker.selected.len();
                bot.save_picker_state();
                bot.api()
                    .answer_callback_query(cb_id, Some(&format!("{count} selected")))
                    .await?;
            } else {
                bot.api()
                    .answer_callback_query(cb_id, Some("Expired"))
                    .await?;
            }
        }
        "confirm" => {
            // Peek first — if nothing selected, keep the picker alive so the user can retry.
            let has_selection = bot
                .ms_picker_mut(prefix, aid)
                .map(|p| !p.selected.is_empty())
                .unwrap_or(false);
            if !has_selection {
                bot.api()
                    .answer_callback_query(
                        cb_id,
                        Some("No items selected \u{2014} tap items first"),
                    )
                    .await?;
                return Ok(());
            }
            // Now consume the picker.
            let picker = match prefix {
                "ms_cancel" => bot.take_cancel_picker(aid),
                "ms_delete" => bot.take_delete_picker(aid),
                _ => None,
            };
            let ids: Vec<String> = picker
                .map(|p| {
                    p.selected
                        .iter()
                        .filter_map(|&idx| p.items.get(idx).map(|i| i.id.clone()))
                        .collect()
                })
                .unwrap_or_default();

            bot.api()
                .answer_callback_query(cb_id, Some("Processing\u{2026}"))
                .await?;
            if let Err(e) = bot
                .edit_message(cid, mid, "\u{23f3} Processing\u{2026}")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }

            use crate::callback_actions;
            match prefix {
                "ms_cancel" => callback_actions::cancel_items(bot, cid, &ids).await?,
                "ms_delete" => callback_actions::delete_items(bot, cid, &ids).await?,
                _ => {}
            }
        }
        "cancel" => {
            match prefix {
                "ms_cancel" => {
                    bot.take_cancel_picker(aid);
                }
                "ms_delete" => {
                    bot.take_delete_picker(aid);
                }
                _ => {}
            }
            bot.api()
                .answer_callback_query(cb_id, Some("Dismissed"))
                .await?;
            if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Dismissed").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}

// ── Cron action ──────────────────────────────────────────────────────

async fn handle_cron_act(
    bot: &TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    if action == "skip" {
        bot.api()
            .answer_callback_query(cb_id, Some("Skipped"))
            .await?;
        if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Skipped").await {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
    } else if let Some(pid_str) = action.strip_prefix("kill_") {
        bot.api()
            .answer_callback_query(cb_id, Some("Killing\u{2026}"))
            .await?;
        if let Err(e) = bot.edit_message(cid, mid, "\u{23f3} Killing\u{2026}").await {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
        crate::callback_actions::kill_worker(bot, cid, mid, pid_str).await?;
    } else {
        bot.api().answer_callback_query(cb_id, None).await?;
    }
    Ok(())
}

// ── Captain learn ───────────────────────────────────────────────────

async fn handle_captain_learn(bot: &TelegramBot, cb_id: &str, cid: &str) -> Result<()> {
    bot.api()
        .answer_callback_query(cb_id, Some("Running learn cycle\u{2026}"))
        .await?;
    match bot
        .gw()
        .post("/api/knowledge/learn", &serde_json::json!({}))
        .await
    {
        Ok(resp) => {
            let summary = resp
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("done");
            let esc = mando_shared::escape_html(summary);
            bot.send_html(cid, &format!("\u{2705} Learn complete:\n{esc}"))
                .await?;
        }
        Err(e) => {
            bot.send_html(cid, &format!("\u{274c} Learn failed: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Answer / Retry / View callbacks ─────────────────────────────────

async fn handle_answer_cb(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some("Use /answer command"))
        .await?;
    bot.send_html(
        cid,
        &format!("Reply with:\n<code>/answer {item_id} your answer here</code>"),
    )
    .await?;
    Ok(())
}

async fn handle_retry_cb(bot: &TelegramBot, parts: &[&str], cb_id: &str, cid: &str) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some("Retrying\u{2026}"))
        .await?;
    crate::callback_actions::retry_item(bot, cid, item_id).await
}

async fn handle_view_cb(bot: &TelegramBot, parts: &[&str], cb_id: &str, cid: &str) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some("Loading\u{2026}"))
        .await?;
    crate::commands::timeline::handle(bot, cid, item_id).await
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn callback_data_parsing() {
        let data = "reopen:pick:abc12345:3";
        let parts: Vec<&str> = data.split(':').collect();
        assert_eq!(parts[0], "reopen");
        assert_eq!(parts[1], "pick");
        assert_eq!(parts[2], "abc12345");
        assert_eq!(parts[3], "3");
    }

    #[test]
    fn callback_data_short() {
        let data = "ops:end";
        let parts: Vec<&str> = data.split(':').collect();
        assert_eq!(parts[0], "ops");
        assert_eq!(parts[1], "end");
    }

    #[test]
    fn callback_prefix_dispatch() {
        let prefixes = [
            "swarm_input",
            "reopen",
            "handoff",
            "todo_confirm",
            "todo_project",
            "merge",
            "accept",
            "answer",
            "retry",
            "view",
            "ms_cancel",
            "ms_delete",
            "rework",
            "cron_act",
            "captain_learn",
            "knowledge",
            "ops",
            "ask",
            "dg",
        ];
        assert_eq!(prefixes.len(), 19);
    }
}
