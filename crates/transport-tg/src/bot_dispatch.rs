//! Command dispatch, plain-text routing, and command registration.
//!
//! Extracted from bot.rs for file length.

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use crate::bot::TelegramBot;
use crate::bot_helpers::bc;
use crate::commands;

pub(crate) const REGISTERED_COMMANDS: &[(&str, &str)] = &[
    ("start", "Show available commands"),
    ("todo", "Add tasks"),
    ("action", "Actions on a task"),
    ("tasks", "Show task list"),
    ("stop", "Stop all active workers"),
    ("timeline", "Task lifecycle timeline + Q&A history"),
    ("triage", "Rank pending-review PRs by merge readiness"),
    ("health", "System health + active workers"),
    ("scout_add", "Add URL to Scout"),
    ("scout_research", "AI-powered link discovery on a topic"),
    ("scout_list", "List scout items"),
    ("scout_saved", "View saved scout items"),
    ("scout", "Review processed items (swipe)"),
];

impl TelegramBot {
    #[tracing::instrument(skip_all, fields(module = "telegram", command = command))]
    pub(crate) async fn dispatch_command(
        &mut self,
        chat_id: &str,
        command: &str,
        args: &str,
    ) -> Result<()> {
        debug!("/{command} args={args:?}");
        match command {
            "start" | "help" => commands::help::handle(self, chat_id, args).await,
            "todo" => commands::todo::handle(self, chat_id, args).await,
            "tasks" => commands::status::handle(self, chat_id, args).await,
            "action" => commands::action::handle(self, chat_id, args).await,
            "timeline" | "history" => commands::timeline::handle(self, chat_id, args).await,
            "triage" => commands::triage::handle(self, chat_id, args).await,
            "stop" => commands::stop::handle(self, chat_id, args).await,
            "health" | "workers" => commands::health::handle(self, chat_id, args).await,
            // Scout commands
            "scout_add" => crate::assistant::commands::cmd_addlink(self, chat_id, args).await,
            "scout_research" => crate::assistant::commands::cmd_research(self, chat_id, args).await,
            "scout_list" => crate::assistant::commands::cmd_simplelist(self, chat_id, args).await,
            "scout_saved" => {
                crate::assistant::commands::cmd_simplelist(self, chat_id, "saved").await
            }
            "scout" if args.is_empty() => {
                crate::assistant::commands::cmd_scout(self, chat_id).await
            }
            "scout" => {
                crate::assistant::commands::send_help(self, chat_id, "/scout takes no arguments.")
                    .await
            }
            _ => {
                debug!("Unknown: /{command}");
                Ok(())
            }
        }
    }

    pub(crate) async fn handle_plain_text(
        &mut self,
        chat_id: &str,
        text: &str,
        message: &Value,
    ) -> Result<()> {
        if self.pending_todo.remove(chat_id) {
            return commands::todo::execute_todo(self, chat_id, text).await;
        }
        if let Some((item_id, title)) = self.pending_reopen.remove(chat_id) {
            return crate::callback_actions::reopen_with_feedback(
                self, chat_id, &item_id, &title, text,
            )
            .await;
        }
        if let Some((item_id, title)) = self.pending_rework.remove(chat_id) {
            return crate::callback_actions::rework_with_feedback(
                self, chat_id, &item_id, &title, text,
            )
            .await;
        }
        if let Some((item_id, _title)) = self.pending_nudge.remove(chat_id) {
            let mid = self
                .send_loading(
                    chat_id,
                    &format!(
                        "\u{23f3} Nudging #{}...",
                        crate::telegram_format::escape_html(&item_id)
                    ),
                )
                .await?;
            let gw = self.gw().clone();
            match gw
                .post(
                    crate::gateway_paths::CAPTAIN_NUDGE,
                    &serde_json::json!({"item_id": item_id, "message": text}),
                )
                .await
            {
                Ok(resp) => {
                    let worker = resp["worker"].as_str().unwrap_or("worker");
                    self.edit_message(
                        chat_id,
                        mid,
                        &format!(
                            "\u{1f4e3} Nudged {} for #{}",
                            crate::telegram_format::escape_html(worker),
                            crate::telegram_format::escape_html(&item_id),
                        ),
                    )
                    .await?;
                }
                Err(e) => {
                    self.edit_message(
                        chat_id,
                        mid,
                        &format!(
                            "\u{274c} Nudge failed for #{}: {}",
                            crate::telegram_format::escape_html(&item_id),
                            crate::telegram_format::escape_html(&e.to_string()),
                        ),
                    )
                    .await?;
                }
            }
            return Ok(());
        }
        if commands::action::handle_input_text(self, chat_id, text).await? {
            return Ok(());
        }
        if commands::action::handle_ask_text(self, chat_id, text).await? {
            return Ok(());
        }
        // Scout act session: take atomically to avoid TOCTOU
        if !text.is_empty() {
            if let Some(session) = self.act_sessions.remove(chat_id) {
                return crate::assistant::act::execute_act(
                    self,
                    chat_id,
                    session.item_id,
                    &session.project,
                    Some(text),
                )
                .await;
            }
        }
        // Scout QA session: intercept plain text as questions
        if self.qa_sessions.contains_key(chat_id) {
            return self.handle_qa_text(chat_id, text).await;
        }
        // Implicit URL detection (scout)
        crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
    }

    pub(crate) async fn register_commands(&self) {
        let cmds = REGISTERED_COMMANDS
            .iter()
            .map(|(command, description)| bc(command, description))
            .collect();
        if let Err(e) = self.api.set_my_commands(cmds).await {
            warn!("Failed to register bot commands: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::REGISTERED_COMMANDS;

    #[test]
    fn registered_commands_cover_contract_subset() {
        let contract: Value =
            serde_json::from_str(include_str!("../../../contracts/capabilities.json")).unwrap();
        let names: std::collections::HashSet<&str> =
            REGISTERED_COMMANDS.iter().map(|(name, _)| *name).collect();

        assert!(
            contract["captain"].get("tasks").is_some(),
            "missing tasks in contract"
        );
        assert!(names.contains("tasks"), "missing /tasks registration");

        // accept/nudge are now reached via /action, not individual registrations
        for command in ["triage", "stop"] {
            assert!(
                contract["captain"].get(command).is_some(),
                "missing {command} in contract"
            );
            assert!(names.contains(command), "missing /{command} registration");
        }

        for command in ["scout_add", "scout_research", "scout"] {
            let capability = match command {
                "scout_add" => "add",
                "scout_research" => "research",
                "scout" => "read",
                other => other,
            };
            assert!(
                contract["scout"].get(capability).is_some(),
                "missing {capability} in contract"
            );
            assert!(names.contains(command), "missing /{command} registration");
        }
    }
}
