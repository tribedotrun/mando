//! Command dispatch, plain-text routing, and command registration.
//!
//! Extracted from bot.rs for file length.

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use crate::bot::TelegramBot;
use crate::bot_helpers::bc;
use crate::commands;

impl TelegramBot {
    #[tracing::instrument(skip_all, fields(module = "telegram", command = command))]
    pub(crate) async fn dispatch_command(
        &mut self,
        chat_id: &str,
        command: &str,
        args: &str,
        is_group: bool,
    ) -> Result<()> {
        debug!("/{command} args={args:?}");
        match command {
            "start" | "help" => commands::help::handle(self, chat_id, args, is_group).await,
            "todo" => commands::todo::handle(self, chat_id, args).await,
            "status" => commands::status::handle(self, chat_id, args).await,
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
            "health" => commands::health::handle(self, chat_id, args).await,
            // Scout commands
            "addlink" => {
                crate::assistant::commands::cmd_addlink(self, chat_id, args, is_group).await
            }
            "research" => crate::assistant::commands::cmd_research(self, chat_id, args).await,
            "list" => crate::assistant::commands::cmd_list(self, chat_id, args).await,
            "simplelist" => crate::assistant::commands::cmd_simplelist(self, chat_id, args).await,
            "saved" => crate::assistant::commands::cmd_list(self, chat_id, "saved").await,
            "scout" if args.is_empty() => {
                crate::assistant::commands::cmd_scout(self, chat_id, is_group).await
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
        if self.pending_todo.remove(chat_id).is_some() {
            return commands::todo::execute_todo(self, chat_id, text).await;
        }
        if let Some((item_id, title)) = self.pending_reopen.remove(chat_id) {
            return crate::callback_actions::reopen_with_feedback(
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
        let cmds = vec![
            bc("start", "Show available commands"),
            bc("todo", "Add backlog items"),
            bc("reopen", "Reopen done/failed item"),
            bc("handoff", "Hand off item to human"),
            bc("adopt", "Adopt human's worktree"),
            bc("input", "Clarify items"),
            bc("captain", "Run captain tick"),
            bc("status", "Show backlog status"),
            bc("cron", "Manage cron jobs"),
            bc("answer", "Answer clarifier questions"),
            bc("retry", "Retry errored captain review"),
            bc("cancel", "Cancel a backlog item"),
            bc("rework", "Rework item"),
            bc("delete", "Delete a backlog item"),
            bc("ops", "Ops copilot"),
            bc("ask", "Q&A on completed items"),
            bc("sessions", "List CC sessions"),
            bc("timeline", "Item lifecycle timeline"),
            bc("prsummary", "Show PR description"),
            bc("knowledge", "Approve knowledge lessons"),
            bc("triage", "Rank pending-review PRs by merge readiness"),
            bc("health", "System health (daemon, workers, config)"),
            bc("history", "Show ask history for an item"),
            // Scout commands
            bc("addlink", "Add URL to scout queue"),
            bc("research", "AI-powered link discovery on a topic"),
            bc("list", "List scout items with summaries"),
            bc("simplelist", "List scout items (compact)"),
            bc("saved", "View saved scout items"),
            bc("scout", "Review processed items (swipe)"),
        ];
        if let Err(e) = self.api.set_my_commands(cmds).await {
            warn!("Failed to register bot commands: {e}");
        }
    }
}
