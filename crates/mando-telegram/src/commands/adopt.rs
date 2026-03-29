//! `/adopt <worktree> <title>` — captain adopts human's worktree.

use crate::bot::TelegramBot;
use crate::gateway_paths;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;

/// Handle `/adopt <worktree_path> <title...>`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();

    if parts.len() < 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        bot.send_html(
            chat_id,
            "Usage: /adopt &lt;worktree_path&gt; &lt;title...&gt;\n\n\
             Example:\n\
             /adopt /path/to/worktree Fix auth flow\n\n\
             Path must come first (no spaces). Everything after is the title.\n\
             Tip: <code>mando captain adopt</code> from the worktree is easier \
             (auto-detects path/branch).",
        )
        .await?;
        return Ok(());
    }

    let wt_path = parts[0].trim();
    let title = parts[1].trim();

    // Check worktree exists.
    let wt = std::path::Path::new(wt_path);
    if !wt.exists() || !wt.join(".git").exists() {
        bot.send_html(
            chat_id,
            &format!(
                "\u{26a0}\u{fe0f} Not a git worktree: <code>{}</code>",
                escape_html(wt_path)
            ),
        )
        .await?;
        return Ok(());
    }

    // Detect branch from worktree.
    let branch = detect_branch(wt_path).await.unwrap_or_default();
    if branch.is_empty() {
        bot.send_html(chat_id, "\u{26a0}\u{fe0f} Could not detect branch.")
            .await?;
        return Ok(());
    }

    // Create task entry via gateway (note goes into item context).
    let ack = bot
        .send_html(chat_id, "\u{2699}\u{fe0f} Adopting worktree\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let note = "Continue from current state. Run tests, fix failures, create PR.";
    let result = bot
        .gw()
        .post(
            gateway_paths::CAPTAIN_ADOPT,
            &serde_json::json!({
                "path": wt_path,
                "title": title,
                "branch": branch,
                "note": note,
            }),
        )
        .await;

    match result {
        Ok(val) => {
            let id = val["id"]
                .as_i64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".into());
            bot.edit_message(
                chat_id,
                ack_mid,
                &format!(
                    "\u{2705} Adopted #{id}: {}\n\
                     Branch: <code>{}</code>\n\
                     Worktree: <code>{}</code>\n\
                     Brief: <code>.ai/briefs/adopt-handoff.md</code>\n\
                     Captain will pick this up on next tick.",
                    escape_html(title),
                    escape_html(&branch),
                    escape_html(wt_path),
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                ack_mid,
                &format!("\u{274c} Adopt failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}

/// Detect the current branch in a worktree via `git rev-parse`.
async fn detect_branch(wt_path: &str) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(wt_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        None
    } else {
        Some(branch)
    }
}
