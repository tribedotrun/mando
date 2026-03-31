use std::sync::Arc;

use anyhow::Result;
use tracing::warn;

use mando_shared::telegram_format::escape_html;

use super::commands::send_html;
use super::formatting::{format_swipe_card, swipe_card_kb};
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn cmd_process(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let body = match args.trim().trim_start_matches('#') {
        "" => serde_json::json!({}),
        value => match value.parse::<i64>() {
            Ok(id) => serde_json::json!({ "id": id }),
            Err(_) => {
                send_html(
                    bot,
                    chat_id,
                    "Usage: /process [item_id]\nExample: /process 42",
                )
                .await?;
                return Ok(());
            }
        },
    };

    let sent = send_html(bot, chat_id, "⏳ Processing Scout…").await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    match bot.gw().post(paths::SCOUT_PROCESS, &body).await {
        Ok(resp) => {
            let processed = resp["processed"].as_u64().unwrap_or(0);
            if let Err(e) = bot
                .api
                .edit_message_text(
                    chat_id,
                    message_id,
                    &format!("✅ Processed {} Scout item(s).", processed),
                    None,
                    None,
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        Err(e) => {
            if let Err(send_err) = bot
                .api
                .edit_message_text(
                    chat_id,
                    message_id,
                    &format!("❌ Process failed: {}", escape_html(&e.to_string())),
                    Some("HTML"),
                    None,
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %send_err, "message send failed");
            }
        }
    }

    Ok(())
}

/// Process a single item, editing `message_id` in place with progress then the final card.
pub async fn auto_process_single(bot: &TelegramBot, chat_id: &str, message_id: i64, id: i64) {
    if let Err(e) = bot
        .api
        .edit_message_text(
            chat_id,
            message_id,
            "\u{23f3} Processing\u{2026}",
            None,
            None,
        )
        .await
    {
        tracing::warn!(module = "telegram", error = %e, "message send failed");
    }

    let body = serde_json::json!({"id": id});
    match bot.gw().post(paths::SCOUT_PROCESS, &body).await {
        Ok(_) => match bot.gw().get(&paths::scout_item(id)).await {
            Ok(item) => {
                let summary = item["summary"].as_str();
                let text = format_swipe_card(&item, summary);
                let tg_url = item["telegraphUrl"].as_str();
                let kb = swipe_card_kb(id, tg_url);
                if let Err(e) = bot
                    .api
                    .edit_message_text(chat_id, message_id, &text, Some("HTML"), Some(kb))
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
            Err(_) => {
                if let Err(e) = bot
                    .api
                    .edit_message_text(
                        chat_id,
                        message_id,
                        &format!("\u{2705} #{id} processed (couldn\u{2019}t load card)"),
                        None,
                        None,
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
        },
        Err(e) => {
            warn!(item_id = id, error = %e, "auto-process failed");
            if let Err(e) = bot
                .api
                .edit_message_text(
                    chat_id,
                    message_id,
                    &format!(
                        "\u{274c} #{id} processing failed: {}",
                        escape_html(&e.to_string())
                    ),
                    Some("HTML"),
                    None,
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
    }
}

/// Process multiple items concurrently, editing `message_id` with a final summary.
/// Settles into the first successfully processed item's swipe card.
pub async fn auto_process_batch(bot: &TelegramBot, chat_id: &str, message_id: i64, ids: &[i64]) {
    use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
    use tokio::task::JoinSet;

    let total = ids.len();
    if let Err(e) = bot
        .api
        .edit_message_text(
            chat_id,
            message_id,
            &format!("\u{23f3} Processing {total} items\u{2026}"),
            None,
            None,
        )
        .await
    {
        tracing::warn!(module = "telegram", error = %e, "message send failed");
    }

    let fail = Arc::new(AtomicU32::new(0));
    let first_ok_id = Arc::new(AtomicI64::new(-1));
    let mut tasks = JoinSet::new();

    for &id in ids {
        let gw = bot.gw().clone();
        let fail = Arc::clone(&fail);
        let first_ok_id = Arc::clone(&first_ok_id);
        tasks.spawn(async move {
            let body = serde_json::json!({"id": id});
            match gw.post(paths::SCOUT_PROCESS, &body).await {
                Ok(_) => {
                    let _ =
                        first_ok_id.compare_exchange(-1, id, Ordering::AcqRel, Ordering::Relaxed);
                }
                Err(e) => {
                    fail.fetch_add(1, Ordering::Relaxed);
                    warn!(item_id = id, error = %e, "auto-process failed");
                }
            }
        });
    }

    while let Some(result) = tasks.join_next().await {
        if let Err(e) = result {
            warn!(error = %e, "batch process task panicked");
            fail.fetch_add(1, Ordering::Relaxed);
        }
    }

    let fail = fail.load(Ordering::Relaxed);
    let ok_id = first_ok_id.load(Ordering::Relaxed);

    let msg = if ok_id < 0 {
        format!("\u{274c} All {total} items failed to process")
    } else {
        match bot.gw().get(&paths::scout_item(ok_id)).await {
            Ok(item) => {
                let summary = item["summary"].as_str();
                let mut text = format_swipe_card(&item, summary);
                if fail > 0 {
                    text.push_str(&format!("\n\n\u{274c} {fail}/{total} failed"));
                }
                let tg_url = item["telegraphUrl"].as_str();
                let kb = swipe_card_kb(ok_id, tg_url);
                if let Err(e) = bot
                    .api
                    .edit_message_text(chat_id, message_id, &text, Some("HTML"), Some(kb))
                    .await
                {
                    warn!(module = "telegram", error = %e, "message send failed");
                }
                return;
            }
            Err(_) => format!(
                "\u{2705} {}/{total} processed (couldn\u{2019}t load card)",
                total as u32 - fail
            ),
        }
    };
    if let Err(e) = bot
        .api
        .edit_message_text(chat_id, message_id, &msg, None, None)
        .await
    {
        warn!(module = "telegram", error = %e, "message send failed");
    }
}
