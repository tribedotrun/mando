//! `/cron` — manage cron jobs via gateway API.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s.trim(), r.trim()),
        None => (args.trim(), ""),
    };

    match sub {
        "" | "list" => list(bot, chat_id).await,
        "add" => add(bot, chat_id, rest).await,
        "enable" => toggle(bot, chat_id, rest, true).await,
        "disable" => toggle(bot, chat_id, rest, false).await,
        "delete" => delete(bot, chat_id, rest).await,
        "test" => test(bot, chat_id, rest).await,
        _ => {
            bot.send_html(
                chat_id,
                "Unknown subcommand. Use: list, add, enable, disable, delete, test",
            )
            .await?;
            Ok(())
        }
    }
}

async fn list(bot: &TelegramBot, chat_id: &str) -> Result<()> {
    let resp = match bot.gw().get("/api/cron").await {
        Ok(r) => r,
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "❌ Failed to load cron jobs: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let jobs = resp["jobs"].as_array().cloned().unwrap_or_default();
    if jobs.is_empty() {
        bot.send_html(chat_id, "No cron jobs configured.").await?;
        return Ok(());
    }

    let mut lines = vec!["⏰ <b>Cron Jobs</b>".to_string()];
    for job in &jobs {
        let enabled = job["enabled"].as_bool().unwrap_or(false);
        let status = if enabled { "✅" } else { "❌" };
        let id = job["id"].as_str().unwrap_or("?");
        let schedule = job["schedule"].as_str().unwrap_or("unknown");
        let name = escape_html(job["name"].as_str().unwrap_or("?"));
        let last = job["last_run"].as_str().unwrap_or("never");
        let next = job["next_run"].as_str().unwrap_or("-");
        lines.push(format!(
            "{status} <b>{name}</b> (<code>{}</code>)\n    <code>{}</code> | last: {} | next: {}",
            escape_html(id),
            escape_html(schedule),
            escape_html(last),
            escape_html(next),
        ));
    }

    bot.send_html(chat_id, &lines.join("\n")).await?;
    Ok(())
}

async fn add(bot: &TelegramBot, chat_id: &str, rest: &str) -> Result<()> {
    // Format: <name> every <interval> <message>
    // Format: <name> cron <expr> <message>
    let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
    if parts.len() < 2 {
        bot.send_html(
            chat_id,
            "Usage: /cron add &lt;name&gt; every &lt;interval&gt; &lt;message&gt;\n\
             or: /cron add &lt;name&gt; cron &lt;expr&gt; &lt;message&gt;",
        )
        .await?;
        return Ok(());
    }

    let name = parts[0];
    let after_name = parts[1].trim();

    let (kind, value, message) = if let Some(rest) = after_name.strip_prefix("every ") {
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            bot.send_html(chat_id, "Missing message after interval.")
                .await?;
            return Ok(());
        }
        ("every", parts[0], parts[1])
    } else if let Some(rest) = after_name.strip_prefix("cron ") {
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            bot.send_html(chat_id, "Missing message after cron expression.")
                .await?;
            return Ok(());
        }
        ("cron", parts[0], parts[1])
    } else {
        bot.send_html(
            chat_id,
            "Expected <code>every</code> or <code>cron</code> after name.",
        )
        .await?;
        return Ok(());
    };

    let body = json!({
        "name": name,
        "schedule_kind": kind,
        "schedule_value": value,
        "message": message,
    });

    match bot.gw().post("/api/cron/add", &body).await {
        Ok(resp) => {
            let id = resp["id"].as_str().unwrap_or("?");
            bot.send_html(
                chat_id,
                &format!(
                    "✅ Created cron job <b>{}</b> (<code>{}</code>)",
                    escape_html(name),
                    escape_html(id)
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(chat_id, &format!("❌ {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

async fn toggle(bot: &TelegramBot, chat_id: &str, id: &str, enabled: bool) -> Result<()> {
    if id.is_empty() {
        bot.send_html(chat_id, "Usage: /cron enable|disable &lt;id&gt;")
            .await?;
        return Ok(());
    }
    let body = json!({ "id": id, "enabled": enabled });
    match bot.gw().post("/api/cron/toggle", &body).await {
        Ok(_) => {
            let verb = if enabled { "enabled" } else { "disabled" };
            bot.send_html(
                chat_id,
                &format!("✅ Job <code>{}</code> {verb}", escape_html(id)),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(chat_id, &format!("❌ {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

async fn delete(bot: &TelegramBot, chat_id: &str, id: &str) -> Result<()> {
    if id.is_empty() {
        bot.send_html(chat_id, "Usage: /cron delete &lt;id&gt;")
            .await?;
        return Ok(());
    }
    let body = json!({ "id": id });
    match bot.gw().post("/api/cron/remove", &body).await {
        Ok(_) => {
            bot.send_html(
                chat_id,
                &format!("✅ Deleted job <code>{}</code>", escape_html(id)),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(chat_id, &format!("❌ {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

async fn test(bot: &TelegramBot, chat_id: &str, id: &str) -> Result<()> {
    if id.is_empty() {
        bot.send_html(chat_id, "Usage: /cron test &lt;id&gt;")
            .await?;
        return Ok(());
    }
    let body = json!({ "id": id });
    match bot.gw().post("/api/cron/run", &body).await {
        Ok(_) => {
            bot.send_html(
                chat_id,
                &format!("✅ Triggered job <code>{}</code>", escape_html(id)),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(chat_id, &format!("❌ {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}
