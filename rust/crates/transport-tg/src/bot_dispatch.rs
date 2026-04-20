//! Command dispatch, plain-text routing, and command registration.
//!
//! Extracted from bot.rs for file length.

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use crate::bot::TelegramBot;
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

/// Single source of truth for a Telegram command. The dispatch match arms in
/// `dispatch_command` must cover every `name` and every entry in `aliases`;
/// `/help` text is generated from the `Public` specs filtered by feature gate.
pub(crate) struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub help_short: &'static str,
    pub visibility: CommandVisibility,
    pub feature_gate: FeatureGate,
    pub section: HelpSection,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpSection {
    Tasks,
    System,
    Scout,
}

pub(crate) const REGISTERED_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "start",
        aliases: &["help"],
        help_short: "Show available commands",
        visibility: CommandVisibility::Hidden,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
    },
    CommandSpec {
        name: "todo",
        aliases: &[],
        help_short: "Add tasks",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
    },
    CommandSpec {
        name: "tasks",
        aliases: &[],
        help_short: "Show task list",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
    },
    CommandSpec {
        name: "action",
        aliases: &[],
        help_short: "Actions on a task",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::Tasks,
    },
    CommandSpec {
        name: "triage",
        aliases: &[],
        help_short: "Rank pending-review PRs",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
    },
    CommandSpec {
        name: "health",
        aliases: &["workers"],
        help_short: "System health + active workers",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
    },
    CommandSpec {
        name: "stop",
        aliases: &[],
        help_short: "Stop all active workers",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
    },
    CommandSpec {
        name: "timeline",
        aliases: &["history"],
        help_short: "Task timeline + Q&A history",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::Always,
        section: HelpSection::System,
    },
    CommandSpec {
        name: "scout_add",
        aliases: &[],
        help_short: "Add URL to Scout",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
    },
    CommandSpec {
        name: "scout_research",
        aliases: &[],
        help_short: "AI-powered link discovery",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
    },
    CommandSpec {
        name: "scout_list",
        aliases: &[],
        help_short: "List scout items",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
    },
    CommandSpec {
        name: "scout_saved",
        aliases: &[],
        help_short: "View saved scout items",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
    },
    CommandSpec {
        name: "scout",
        aliases: &[],
        help_short: "Review processed items (swipe)",
        visibility: CommandVisibility::Public,
        feature_gate: FeatureGate::ScoutEnabled,
        section: HelpSection::Scout,
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
        &mut self,
        chat_id: &str,
        command: &str,
        args: &str,
    ) -> Result<()> {
        debug!("/{command} args={args:?}");
        let Some(spec) = lookup_command(command) else {
            debug!("Unknown: /{command}");
            return Ok(());
        };
        match spec.name {
            "start" => commands::help::handle(self, chat_id, args).await,
            "todo" => commands::todo::handle(self, chat_id, args).await,
            "tasks" => commands::status::handle(self, chat_id, args).await,
            "action" => commands::action::handle(self, chat_id, args).await,
            "timeline" => commands::timeline::handle(self, chat_id, args).await,
            "triage" => commands::triage::handle(self, chat_id, args).await,
            "stop" => commands::stop::handle(self, chat_id, args).await,
            "health" => commands::health::handle(self, chat_id, args).await,
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
            other => {
                debug!("Registered command /{other} has no dispatch arm — did you forget to wire it up after adding it to REGISTERED_COMMANDS?");
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
                .post_typed::<_, api_types::NudgeResponse>(
                    crate::gateway_paths::CAPTAIN_NUDGE,
                    &serde_json::json!({"item_id": item_id, "message": text}),
                )
                .await
            {
                Ok(resp) => {
                    let worker = resp.worker.as_deref().unwrap_or("worker");
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
