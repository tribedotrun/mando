//! `/start` and `/help` command handlers.

use crate::bot::TelegramBot;
use crate::bot_dispatch::{CommandSpec, CommandVisibility, HelpSection, REGISTERED_COMMANDS};
use crate::bot_helpers::dm_reply_keyboard;
use anyhow::Result;

fn section_header(section: HelpSection) -> &'static str {
    match section {
        HelpSection::Tasks => "\u{1f99e} <b>Tasks</b>",
        HelpSection::System => "\u{1f3db}\u{fe0f} <b>System</b>",
        HelpSection::Scout => "\u{1f50d} <b>Scout</b>",
    }
}

fn format_line(spec: &CommandSpec) -> String {
    let args = match spec.name {
        "todo" => " [items]",
        "tasks" => " [all]",
        "timeline" => " &lt;id&gt;",
        "scout_add" => " &lt;url&gt;",
        "scout_research" => " &lt;topic&gt;",
        "scout_list" => " [status]",
        _ => "",
    };
    format!(
        "/{name}{args} \u{2014} {desc}",
        name = spec.name,
        desc = spec.help_short
    )
}

fn render_help_text(scout_enabled: bool) -> String {
    let mut sections: Vec<(HelpSection, Vec<String>)> = Vec::new();
    for section in [HelpSection::Tasks, HelpSection::System, HelpSection::Scout] {
        let mut lines: Vec<String> = Vec::new();
        for spec in REGISTERED_COMMANDS {
            if spec.section != section {
                continue;
            }
            if spec.visibility != CommandVisibility::Public {
                continue;
            }
            if !spec.feature_gate.is_enabled(scout_enabled) {
                continue;
            }
            lines.push(format_line(spec));
        }
        if !lines.is_empty() {
            sections.push((section, lines));
        }
    }

    // Insert `/action cancel` sub-command documentation under Tasks,
    // immediately after the `/action` line.
    for (section, lines) in sections.iter_mut() {
        if *section == HelpSection::Tasks {
            if let Some(pos) = lines.iter().position(|l| l.starts_with("/action ")) {
                lines.insert(
                    pos + 1,
                    "/action cancel \u{2014} Close active session".to_string(),
                );
            }
        }
    }

    let mut rendered = String::new();
    for (i, (section, lines)) in sections.iter().enumerate() {
        if i > 0 {
            rendered.push_str("\n\n");
        }
        rendered.push_str(section_header(*section));
        for line in lines {
            rendered.push('\n');
            rendered.push_str(line);
        }
    }
    rendered
}

/// Handle `/start` or `/help`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let scout_enabled = bot.config().read().await.features.scout;
    let keyboard = dm_reply_keyboard(scout_enabled);
    let text = render_help_text(scout_enabled);
    bot.api()
        .send_message(chat_id, &text, Some("HTML"), Some(keyboard), true)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_help_text;
    use crate::bot_dispatch::{CommandVisibility, FeatureGate, REGISTERED_COMMANDS};

    #[test]
    fn help_with_scout_enabled_includes_all_public_commands() {
        let text = render_help_text(true);
        for spec in REGISTERED_COMMANDS {
            if spec.visibility != CommandVisibility::Public {
                continue;
            }
            let needle = format!("/{}", spec.name);
            assert!(
                text.contains(&needle),
                "/help (scout on) missing /{} — full text:\n{text}",
                spec.name
            );
        }
    }

    #[test]
    fn help_with_scout_disabled_omits_scout_commands() {
        let text = render_help_text(false);
        for spec in REGISTERED_COMMANDS {
            if !matches!(spec.feature_gate, FeatureGate::ScoutEnabled) {
                continue;
            }
            let needle = format!("/{} ", spec.name);
            assert!(
                !text.contains(&needle),
                "/help (scout off) leaked scout command /{} — full text:\n{text}",
                spec.name
            );
        }
    }

    #[test]
    fn help_with_scout_disabled_includes_non_scout_public_commands() {
        let text = render_help_text(false);
        for spec in REGISTERED_COMMANDS {
            if spec.visibility != CommandVisibility::Public {
                continue;
            }
            if matches!(spec.feature_gate, FeatureGate::ScoutEnabled) {
                continue;
            }
            let needle = format!("/{}", spec.name);
            assert!(
                text.contains(&needle),
                "/help (scout off) missing non-scout /{} — full text:\n{text}",
                spec.name
            );
        }
    }

    #[test]
    fn hidden_commands_do_not_appear_in_help() {
        let text_on = render_help_text(true);
        let text_off = render_help_text(false);
        for spec in REGISTERED_COMMANDS
            .iter()
            .filter(|s| s.visibility == CommandVisibility::Hidden)
        {
            let needle = format!("/{} ", spec.name);
            assert!(
                !text_on.contains(&needle),
                "/help (scout on) exposed hidden /{}",
                spec.name
            );
            assert!(
                !text_off.contains(&needle),
                "/help (scout off) exposed hidden /{}",
                spec.name
            );
        }
    }
}
