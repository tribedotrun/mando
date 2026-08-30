//! Command dispatch, plain-text routing, and command registration.
//!
//! Extracted from bot.rs for file length.

use anyhow::Result;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, warn};

use crate::bot::{SessionKind, TelegramBot};
use crate::bot_helpers::bc;
use crate::commands;

/// Gate that determines whether a command is surfaced / dispatchable. Kept
/// narrow so drift tests can enumerate per-feature state.
#[derive(Clone, Copy)]
pub(crate) enum FeatureGate {
    Always,
    ScoutEnabled,
}

/// Whether a command appears in `/help`. Hidden commands remain dispatchable
/// (e.g. `start` is the greeting shortcut for `help`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandVisibility {
    Public,
    Hidden,
}

/// Single source of truth for a Telegram command. The handler, `/help` text,
/// and Bot API registration are all derived from this table.
type CommandFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type CommandHandler = for<'a> fn(&'a TelegramBot, &'a str, &'a str) -> CommandFuture<'a>;

pub(crate) struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub help_short: &'static str,
    pub visibility: CommandVisibility,
    pub feature_gate: FeatureGate,
    pub section: HelpSection,
    pub handler: CommandHandler,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpSection {
    Tasks,
    System,
    Scout,
}

fn dispatch_help<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::help::handle(bot, chat_id, args).await })
}

fn dispatch_todo<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::todo::handle(bot, chat_id, args).await })
}

fn dispatch_tasks<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::status::handle(bot, chat_id, args).await })
}

fn dispatch_action<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::action::handle(bot, chat_id, args).await })
}

fn dispatch_triage<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::triage::handle(bot, chat_id, args).await })
}

fn dispatch_health<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::health::handle(bot, chat_id, args).await })
}

fn dispatch_stop<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move { commands::stop::handle(bot, chat_id, args).await })
}

fn dispatch_timeline<'a>(
    bot: &'a TelegramBot,
    chat_id: &'a str,
    args: &'a str,
) -> CommandFuture<'a> {
    Box::pin(async move { commands::timeline::handle(bot, chat_id, args).await })
}

fn dispatch_scout_add<'a>(
    bot: &'a TelegramBot,
    chat_id: &'a str,
    args: &'a str,
) -> CommandFuture<'a> {
    Box::pin(async move { crate::assistant::commands::cmd_addlink(bot, chat_id, args).await })
}

fn dispatch_scout_research<'a>(
    bot: &'a TelegramBot,
    chat_id: &'a str,
    args: &'a str,
) -> CommandFuture<'a> {
    Box::pin(async move { crate::assistant::commands::cmd_research(bot, chat_id, args).await })
}

fn dispatch_scout_list<'a>(
    bot: &'a TelegramBot,
    chat_id: &'a str,
    args: &'a str,
) -> CommandFuture<'a> {
    Box::pin(async move { crate::assistant::commands::cmd_simplelist(bot, chat_id, args).await })
}

fn dispatch_scout_saved<'a>(
    bot: &'a TelegramBot,
    chat_id: &'a str,
    _args: &'a str,
) -> CommandFuture<'a> {
    Box::pin(async move { crate::assistant::commands::cmd_simplelist(bot, chat_id, "saved").await })
}

fn dispatch_scout<'a>(bot: &'a TelegramBot, chat_id: &'a str, args: &'a str) -> CommandFuture<'a> {
    Box::pin(async move {
        if args.is_empty() {
            crate::assistant::commands::cmd_scout(bot, chat_id).await
        } else {
            crate::assistant::commands::send_help(bot, chat_id, "/scout takes no arguments.").await
        }
    })
}

pub(crate) const REGISTERED_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "start",
        aliases: &["help"],
        help_short: "Show available commands",
        visibility: CommandVisibility::Hidden,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
        handler: dispatch_help,
    },
    CommandSpec {
        name: "todo",
        aliases: &[],
        help_short: "Add a task",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
        handler: dispatch_todo,
    },
    CommandSpec {
        name: "tasks",
        aliases: &[],
        help_short: "Show task list",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
        handler: dispatch_tasks,
    },
    CommandSpec {
        name: "action",
        aliases: &[],
        help_short: "Actions on a task",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
        handler: dispatch_action,
    },
    CommandSpec {
        name: "triage",
        aliases: &[],
        help_short: "Rank pending-review PRs",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
        handler: dispatch_triage,
    },
    CommandSpec {
        name: "health",
        aliases: &["workers"],
        help_short: "System health + active workers",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
        handler: dispatch_health,
    },
    CommandSpec {
        name: "stop",
        aliases: &[],
        help_short: "Stop one task (stop [id]) or drain all workers (stop)",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
        handler: dispatch_stop,
    },
    CommandSpec {
        name: "timeline",
        aliases: &["history"],
        help_short: "Task timeline + Q&A history",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
        handler: dispatch_timeline,
    },
    CommandSpec {
        name: "scout_add",
        aliases: &[],
        help_short: "Add URL to Scout",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
        handler: dispatch_scout_add,
    },
    CommandSpec {
        name: "scout_research",
        aliases: &[],
        help_short: "AI-powered link discovery",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
        handler: dispatch_scout_research,
    },
    CommandSpec {
        name: "scout_list",
        aliases: &[],
        help_short: "List scout items",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
        handler: dispatch_scout_list,
    },
    CommandSpec {
        name: "scout_saved",
        aliases: &[],
        help_short: "View saved scout items",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
        handler: dispatch_scout_saved,
    },
    CommandSpec {
        name: "scout",
        aliases: &[],
        help_short: "Review processed items (swipe)",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
        handler: dispatch_scout,
    },
];

/// Look up a spec by canonical name or alias. Returned spec's `name` is the
/// canonical form; callers can use that to route dispatch.
pub(crate) fn lookup_command(input: &str) -> Option<&'static CommandSpec> {
    REGISTERED_COMMANDS
        .iter()
        .find(|spec| spec.name == input || spec.aliases.contains(&input))
}

impl FeatureGate {
    pub(crate) fn is_enabled(self, scout_enabled: bool) -> bool {
        match self {
            Self::Always => true,
            Self::ScoutEnabled => scout_enabled,
        }
    }
}

impl TelegramBot {
    #[tracing::instrument(skip_all, fields(module = "telegram", command = command))]
    pub(crate) async fn dispatch_command(
        &self,
        chat_id: &str,
        command: &str,
        args: &str,
    ) -> Result<()> {
        debug!("/{command} args={args:?}");
        let Some(spec) = lookup_command(command) else {
            debug!("Unknown: /{command}");
            return Ok(());
        };
        (spec.handler)(self, chat_id, args).await
    }

    /// Handle a plain-text (non-`/command`) message.
    ///
    /// Reads `reply_to_message.message_id` and routes to the matching pending
    /// session via [`pick_session_for_text`]. With no reply target, the
    /// most-recently-created pending session wins. Empty pendings fall
    /// through to implicit URL detection (scout).
    pub(crate) async fn handle_plain_text(
        &self,
        chat_id: &str,
        text: &str,
        message: &Value,
    ) -> Result<()> {
        let reply_to = message
            .get("reply_to_message")
            .and_then(|m| m.get("message_id"))
            .and_then(|v| v.as_i64());

        match self.pick_session_for_text(chat_id, reply_to).await {
            // Each text-command branch must atomically claim its pending
            // entry: under concurrent dispatch, two plain-text updates from
            // the same chat can both pick the same SessionKind from the
            // snapshot. Only the task whose `take_*` returns `Some` may run
            // the handler; the loser falls through to implicit URL detection
            // so its text is not silently misrouted into the same command.
            Some(SessionKind::PendingTodo) => {
                if self.take_pending_todo(chat_id).await.is_some() {
                    commands::todo::execute_todo(self, chat_id, text).await
                } else {
                    crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
                }
            }
            Some(SessionKind::PendingTimeline) => {
                if self.take_pending_timeline(chat_id).await.is_some() {
                    commands::timeline::execute(self, chat_id, text).await
                } else {
                    crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
                }
            }
            Some(SessionKind::PendingScoutAdd) => {
                if self.take_pending_scout_add(chat_id).await.is_some() {
                    crate::assistant::commands::execute_addlink(self, chat_id, text).await
                } else {
                    crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
                }
            }
            Some(SessionKind::PendingScoutResearch) => {
                if self.take_pending_scout_research(chat_id).await.is_some() {
                    crate::assistant::commands::execute_research(self, chat_id, text).await
                } else {
                    crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
                }
            }
            Some(SessionKind::PendingReopen) => {
                let Some(action) = self.take_pending_reopen(chat_id).await else {
                    return Ok(());
                };
                crate::callback_actions::reopen_with_feedback(
                    self,
                    chat_id,
                    &action.item_id,
                    &action.title,
                    text,
                )
                .await
            }
            Some(SessionKind::PendingRework) => {
                let Some(action) = self.take_pending_rework(chat_id).await else {
                    return Ok(());
                };
                crate::callback_actions::rework_with_feedback(
                    self,
                    chat_id,
                    &action.item_id,
                    &action.title,
                    text,
                )
                .await
            }
            Some(SessionKind::PendingNudge) => {
                let Some(action) = self.take_pending_nudge(chat_id).await else {
                    return Ok(());
                };
                handle_nudge_text(self, chat_id, &action.item_id, text).await
            }
            Some(SessionKind::InputSession) => {
                if commands::action::handle_input_text(self, chat_id, text).await? {
                    return Ok(());
                }
                // Session vanished between snapshot and consume — fall through.
                crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
            }
            Some(SessionKind::AskSession) => {
                if commands::action::handle_ask_text(self, chat_id, text).await? {
                    return Ok(());
                }
                crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
            }
            Some(SessionKind::ActSession) => {
                if !text.is_empty() {
                    if let Some(session) = self.take_act_session(chat_id).await {
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
                crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
            }
            Some(SessionKind::QaSession) => {
                if self.handle_qa_text(chat_id, text).await? {
                    return Ok(());
                }
                // Session vanished between snapshot and consume (e.g.
                // `endqa` on a concurrent task) — fall through so the
                // user's text is not silently dropped.
                crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
            }
            None => {
                // No pending session for this chat — try implicit URL detection.
                crate::assistant::helpers::handle_implicit_addlink(self, chat_id, message).await
            }
        }
    }

    pub(crate) async fn register_commands(&self) {
        let scout_enabled = self.config().read().await.features.scout;
        let cmds = REGISTERED_COMMANDS
            .iter()
            .filter(|spec| {
                spec.visibility == CommandVisibility::Public
                    && spec.feature_gate.is_enabled(scout_enabled)
            })
            .map(|spec| bc(spec.name, spec.help_short))
            .collect();
        if let Err(e) = self.api.set_my_commands(cmds).await {
            warn!("Failed to register bot commands: {e}");
        }
    }
}

/// Send a captain-nudge for the given item with the user's text as the
/// follow-up message. Extracted from the inline body of `handle_plain_text`
/// so the dispatch site reads as a flat match.
async fn handle_nudge_text(
    bot: &TelegramBot,
    chat_id: &str,
    item_id: &str,
    text: &str,
) -> Result<()> {
    let mid = bot
        .send_loading(
            chat_id,
            &format!(
                "\u{23f3} Nudging #{}...",
                crate::telegram_format::escape_html(item_id)
            ),
        )
        .await?;
    match bot
        .gw()
        .post_typed::<_, api_types::NudgeResponse>(
            crate::gateway_paths::CAPTAIN_NUDGE,
            &serde_json::json!({"item_id": item_id, "message": text}),
        )
        .await
    {
        Ok(resp) => {
            let worker = resp.worker.as_deref().unwrap_or("worker");
            bot.edit_message(
                chat_id,
                mid,
                &format!(
                    "\u{1f4e3} Nudged {} for #{}",
                    crate::telegram_format::escape_html(worker),
                    crate::telegram_format::escape_html(item_id),
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                mid,
                &format!(
                    "\u{274c} Nudge failed for #{}: {}",
                    crate::telegram_format::escape_html(item_id),
                    crate::telegram_format::escape_html(&e.to_string()),
                ),
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use std::collections::HashSet;

    use super::{lookup_command, CommandVisibility, FeatureGate, REGISTERED_COMMANDS};

    fn registered_names() -> HashSet<&'static str> {
        REGISTERED_COMMANDS.iter().map(|spec| spec.name).collect()
    }

    #[test]
    fn registered_commands_cover_contract_subset() {
        let contract: Value =
            serde_json::from_str(include_str!("../../../contracts/capabilities.json")).unwrap();
        let names = registered_names();

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

    #[test]
    fn every_canonical_name_is_unique_across_names_and_aliases() {
        let mut seen: HashSet<&str> = HashSet::new();
        for spec in REGISTERED_COMMANDS {
            assert!(
                seen.insert(spec.name),
                "duplicate command name or alias: /{}",
                spec.name
            );
            for alias in spec.aliases {
                assert!(
                    seen.insert(alias),
                    "duplicate command name or alias: /{alias}"
                );
            }
        }
    }

    #[test]
    fn every_alias_resolves_to_its_canonical_spec() {
        for spec in REGISTERED_COMMANDS {
            for alias in spec.aliases {
                let resolved = lookup_command(alias)
                    .unwrap_or_else(|| panic!("/{alias} does not resolve to a CommandSpec"));
                assert_eq!(
                    resolved.name, spec.name,
                    "/{alias} resolves to /{}, expected /{}",
                    resolved.name, spec.name
                );
            }
        }
    }

    #[test]
    fn scout_commands_are_scout_gated() {
        for spec in REGISTERED_COMMANDS {
            let is_scout_name = spec.name.starts_with("scout");
            match spec.feature_gate {
                FeatureGate::ScoutEnabled => assert!(
                    is_scout_name,
                    "/{} has ScoutEnabled gate but non-scout name",
                    spec.name
                ),
                FeatureGate::Always => assert!(
                    !is_scout_name,
                    "/{} is a scout command but not ScoutEnabled-gated",
                    spec.name
                ),
            }
        }
    }

    #[test]
    fn hidden_commands_are_still_dispatchable() {
        for spec in REGISTERED_COMMANDS
            .iter()
            .filter(|s| s.visibility == CommandVisibility::Hidden)
        {
            let resolved = lookup_command(spec.name)
                .unwrap_or_else(|| panic!("hidden command /{} is not dispatchable", spec.name));
            assert_eq!(resolved.name, spec.name);
        }
    }
}
