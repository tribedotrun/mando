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
    ("accept", "Accept a no-PR task"),
    ("reopen", "Reopen done/failed task"),
    ("handoff", "Hand off task to human"),
    ("adopt", "Adopt human's worktree"),
    ("input", "Clarify tasks"),
    ("captain", "Run captain tick"),
    ("status", "Show task list"),
    ("tasks", "Show task list"),
    ("workers", "Show active workers"),
    ("nudge", "Nudge a stuck worker"),
    ("stop", "Stop all active workers"),
    ("journal", "Show captain decision journal"),
    ("patterns", "Review captain patterns"),
    ("cron", "Manage cron jobs"),
    ("answer", "Answer clarifier questions"),
    ("retry", "Retry errored captain review"),
    ("cancel", "Cancel a task"),
    ("rework", "Rework task"),
    ("delete", "Delete a task"),
    ("ops", "Ops copilot"),
    ("ask", "Q&A on completed tasks"),
    ("sessions", "List CC sessions"),
    ("timeline", "Task lifecycle timeline"),
    ("prsummary", "Show PR description"),
    ("knowledge", "Approve knowledge lessons"),
    ("triage", "Rank pending-review PRs by merge readiness"),
    ("health", "System health (daemon, workers, config)"),
    ("history", "Show ask history for a task"),
    ("addlink", "Add URL to Scout"),
    ("research", "AI-powered link discovery on a topic"),
    ("bulkstatus", "Bulk-update Scout items"),
    ("bulkdelete", "Bulk-delete Scout items"),
    ("publish", "Publish a Scout article"),
    ("list", "List scout items with summaries"),
    ("simplelist", "List scout items (compact)"),
    ("saved", "View saved scout items"),
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
            "status" | "tasks" => commands::status::handle(self, chat_id, args).await,
            "captain" => commands::captain::handle(self, chat_id, args).await,
            "input" => commands::input::handle(self, chat_id, args).await,
            "reopen" => commands::reopen::handle(self, chat_id, args).await,
            "rework" => commands::rework::handle(self, chat_id, args).await,
            "cancel" => commands::cancel::handle(self, chat_id, args).await,
            "delete" => commands::delete::handle(self, chat_id, args).await,
            "handoff" => commands::handoff::handle(self, chat_id, args).await,
            "adopt" => commands::adopt::handle(self, chat_id, args).await,
            "answer" => commands::answer::handle(self, chat_id, args).await,
            "retry" => commands::retry::handle(self, chat_id, args).await,
            "ops" => commands::ops::handle(self, chat_id, args).await,
            "ask" => commands::ask::handle(self, chat_id, args).await,
            "cron" => commands::cron::handle(self, chat_id, args).await,
            "sessions" => commands::sessions::handle(self, chat_id, args).await,
            "history" => commands::history::handle(self, chat_id, args).await,
            "timeline" => commands::timeline::handle(self, chat_id, args).await,
            "prsummary" => commands::pr_summary::handle(self, chat_id, args).await,
            "knowledge" => commands::knowledge::handle(self, chat_id, args).await,
            "triage" => commands::triage::handle(self, chat_id, args).await,
            "accept" => commands::accept::handle(self, chat_id, args).await,
            "workers" => commands::workers::handle(self, chat_id, args).await,
            "nudge" => commands::nudge::handle(self, chat_id, args).await,
            "stop" => commands::stop::handle(self, chat_id, args).await,
            "journal" => commands::journal::handle(self, chat_id, args).await,
            "patterns" => commands::patterns::handle(self, chat_id, args).await,
            "health" => commands::health::handle(self, chat_id, args).await,
            // Scout commands
            "addlink" => crate::assistant::commands::cmd_addlink(self, chat_id, args).await,
            "research" => crate::assistant::commands::cmd_research(self, chat_id, args).await,
            "bulkstatus" => crate::assistant::commands::cmd_bulk_status(self, chat_id, args).await,
            "bulkdelete" => crate::assistant::commands::cmd_bulk_delete(self, chat_id, args).await,
            "publish" => crate::assistant::commands::cmd_publish(self, chat_id, args).await,
            "list" => crate::assistant::commands::cmd_list(self, chat_id, args).await,
            "simplelist" => crate::assistant::commands::cmd_simplelist(self, chat_id, args).await,
            "saved" => crate::assistant::commands::cmd_list(self, chat_id, "saved").await,
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
        if commands::input::handle_text(self, chat_id, text).await? {
            return Ok(());
        }
        if commands::ops::handle_text(self, chat_id, text).await? {
            return Ok(());
        }
        if commands::ask::handle_text(self, chat_id, text).await? {
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

        for command in [
            "workers", "triage", "accept", "nudge", "stop", "journal", "patterns",
        ] {
            assert!(
                contract["captain"].get(command).is_some(),
                "missing {command} in contract"
            );
            assert!(names.contains(command), "missing /{command} registration");
        }

        for command in [
            "addlink",
            "research",
            "bulkstatus",
            "bulkdelete",
            "publish",
            "scout",
        ] {
            let capability = match command {
                "addlink" => "add",
                "bulkstatus" => "bulk_update",
                "bulkdelete" => "bulk_delete",
                "publish" => "publish_article",
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
