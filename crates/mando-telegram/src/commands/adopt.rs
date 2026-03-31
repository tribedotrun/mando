//! `/adopt` — captain adopts a human worktree with explicit context.

use anyhow::Result;

use crate::bot::TelegramBot;
use crate::gateway_paths;
use mando_shared::telegram_format::escape_html;

#[derive(Default)]
struct AdoptInput {
    path: String,
    title: String,
    project: Option<String>,
    branch: Option<String>,
    note: Option<String>,
}

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let Some(input) = parse_args(args) else {
        bot.send_html(
            chat_id,
            "Usage: /adopt &lt;worktree_path&gt; &lt;title...&gt; [--project &lt;name&gt;] [--branch &lt;name&gt;] [--note &lt;text&gt;]\n\n\
             Example:\n\
             /adopt /path/to/worktree Fix auth flow --project mando --note ship tests first",
        )
        .await?;
        return Ok(());
    };

    let wt = std::path::Path::new(&input.path);
    if !wt.exists() || !wt.join(".git").exists() {
        bot.send_html(
            chat_id,
            &format!(
                "⚠️ Not a git worktree: <code>{}</code>",
                escape_html(&input.path)
            ),
        )
        .await?;
        return Ok(());
    }

    let branch = match input.branch.clone().or_else(|| detect_branch(&input.path)) {
        Some(branch) => branch,
        None => {
            bot.send_html(
                chat_id,
                "⚠️ Could not detect branch. Pass --branch explicitly.",
            )
            .await?;
            return Ok(());
        }
    };

    let note = input
        .note
        .as_deref()
        .unwrap_or("Continue from current state. Run tests, fix failures, create PR.");

    let ack = bot.send_html(chat_id, "⚙️ Adopting worktree…").await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let result = bot
        .gw()
        .post(
            gateway_paths::CAPTAIN_ADOPT,
            &serde_json::json!({
                "path": input.path,
                "title": input.title,
                "branch": branch,
                "project": input.project,
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
                    "✅ Adopted #{id}: {}\n\
                     Branch: <code>{}</code>\n\
                     Worktree: <code>{}</code>\n\
                     Note: {}\n\
                     Captain will pick this up on next tick.",
                    escape_html(&input.title),
                    escape_html(&branch),
                    escape_html(&input.path),
                    escape_html(note),
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                ack_mid,
                &format!("❌ Adopt failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}

fn parse_args(args: &str) -> Option<AdoptInput> {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let path = tokens.first()?.trim().to_string();
    if path.is_empty() {
        return None;
    }

    let mut input = AdoptInput {
        path,
        ..AdoptInput::default()
    };
    let mut title_tokens = Vec::new();
    let mut idx = 1usize;
    while idx < tokens.len() {
        match tokens[idx] {
            "--project" => {
                idx += 1;
                input.project = tokens.get(idx).map(|value| (*value).to_string());
            }
            "--branch" => {
                idx += 1;
                input.branch = tokens.get(idx).map(|value| (*value).to_string());
            }
            "--note" => {
                idx += 1;
                if idx >= tokens.len() {
                    return None;
                }
                input.note = Some(tokens[idx..].join(" "));
                break;
            }
            token => title_tokens.push(token.to_string()),
        }
        idx += 1;
    }

    input.title = title_tokens.join(" ").trim().to_string();
    if input.title.is_empty() {
        return None;
    }
    Some(input)
}

fn detect_branch(wt_path: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(wt_path)
        .output()
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

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn parses_explicit_adopt_context() {
        let parsed = parse_args(
            "/tmp/worktree Fix auth flow --project mando --branch feat-x --note ship tests first",
        )
        .unwrap();
        assert_eq!(parsed.path, "/tmp/worktree");
        assert_eq!(parsed.title, "Fix auth flow");
        assert_eq!(parsed.project.as_deref(), Some("mando"));
        assert_eq!(parsed.branch.as_deref(), Some("feat-x"));
        assert_eq!(parsed.note.as_deref(), Some("ship tests first"));
    }
}
