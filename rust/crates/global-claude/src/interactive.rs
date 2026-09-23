//! Provider-owned interactive launch arguments and SessionStart translation.
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

/// Claude's external hook input is extensible; only these fields cross into Mando.
#[derive(Deserialize)]
pub struct InteractiveStart {
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub to_model: Option<String>,
    pub hook_event_name: String,
}

impl InteractiveStart {
    pub fn parse(input: &[u8]) -> Result<Self> {
        let mut event: Self =
            serde_json::from_slice(input).context("invalid Claude SessionStart input")?;
        if event.session_id.is_empty() || event.cwd.is_empty() {
            bail!("Claude SessionStart omitted its session identity or directory");
        }
        if !matches!(
            event.hook_event_name.as_str(),
            "SessionStart" | "CwdChanged" | "PostModelSwitch"
        ) {
            bail!("unsupported Claude launcher hook event");
        }
        if event.to_model.is_some() {
            event.model = event.to_model.take();
        }
        Ok(event)
    }
}

pub struct InteractiveLaunch {
    initial: Vec<String>,
    resumed: Vec<String>,
    pub model: Option<String>,
    settings: Value,
}

impl InteractiveLaunch {
    /// Retain flags while removing one-shot selectors and prompts from subsequent launches.
    pub fn prepare(arguments: &[String], hook_command: &str) -> Result<Self> {
        let mut initial = Vec::new();
        let mut resumed = Vec::new();
        let mut settings = json!({});
        let mut seen_settings = false;
        let mut model = None;
        let mut setting_sources = None;
        let mut args = arguments.iter().peekable();
        while let Some(arg) = args.next() {
            let (flag, inline) = arg
                .split_once('=')
                .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
            if matches!(
                flag,
                "--bare"
                    | "--safe-mode"
                    | "--bg"
                    | "--background"
                    | "--cloud"
                    | "--environment"
                    | "--teleport"
                    | "--worktree"
                    | "-w"
                    | "--tmux"
                    | "--no-session-persistence"
                    | "--print"
                    | "-p"
            ) {
                bail!("{flag} is not supported by the interactive profile-switching launcher");
            }
            if flag == "--settings" {
                if seen_settings {
                    bail!("supply --settings only once");
                }
                seen_settings = true;
                let value = inline
                    .map(str::to_owned)
                    .or_else(|| args.next().cloned())
                    .context("--settings requires a value")?;
                let text = if value.trim_start().starts_with('{') {
                    value
                } else {
                    std::fs::read_to_string(&value).context("read Claude launch settings")?
                };
                settings = serde_json::from_str(&text).context("parse Claude launch settings")?;
                continue;
            }
            if matches!(flag, "--continue" | "-c" | "--fork-session") {
                initial.push(arg.clone());
                continue;
            }
            if matches!(flag, "--resume" | "-r" | "--from-pr" | "--session-id") {
                initial.push(arg.clone());
                if inline.is_none() && args.peek().is_some_and(|v| !v.starts_with('-')) {
                    if let Some(value) = args.next() {
                        initial.push(value.clone());
                    }
                }
                continue;
            }
            if flag == "--" {
                initial.extend(args.cloned());
                break;
            }
            if !arg.starts_with('-') {
                // A positional prompt is used only on the initial launch.
                initial.push(arg.clone());
                continue;
            }
            initial.push(arg.clone());
            resumed.push(arg.clone());
            let required = matches!(
                flag,
                "--agent"
                    | "--agents"
                    | "--append-system-prompt"
                    | "--append-system-prompt-file"
                    | "--autocompact"
                    | "--debug-file"
                    | "--effort"
                    | "--fallback-model"
                    | "--model"
                    | "-n"
                    | "--name"
                    | "--permission-mode"
                    | "--plugin-dir"
                    | "--plugin-url"
                    | "--remote-control-session-name-prefix"
                    | "--setting-sources"
                    | "--system-prompt"
                    | "--system-prompt-file"
                    | "--system-prompt-snapshot"
            );
            let optional = matches!(
                flag,
                "--debug" | "-d" | "--prompt-suggestions" | "--remote-control"
            );
            let multiple = matches!(
                flag,
                "--add-dir"
                    | "--allowedTools"
                    | "--allowed-tools"
                    | "--disallowedTools"
                    | "--disallowed-tools"
                    | "--betas"
                    | "--file"
                    | "--mcp-config"
                    | "--tools"
            );
            let boolean = matches!(
                flag,
                "--allow-dangerously-skip-permissions"
                    | "--dangerously-skip-permissions"
                    | "--ax-screen-reader"
                    | "--brief"
                    | "--chrome"
                    | "--exclude-dynamic-system-prompt-sections"
                    | "--disable-slash-commands"
                    | "--help"
                    | "-h"
                    | "--ide"
                    | "--no-chrome"
                    | "--restricted"
                    | "--strict-mcp-config"
                    | "--verbose"
                    | "--version"
                    | "-v"
            );
            if !(required || optional || multiple || boolean) {
                bail!("unsupported Claude option {flag}; refusing to guess its resume semantics");
            }
            if flag == "--model" {
                model = inline.map(str::to_owned);
            }
            if flag == "--setting-sources" {
                setting_sources = inline.map(str::to_owned);
            }
            if inline.is_none() && required {
                let value = args
                    .next()
                    .with_context(|| format!("{flag} requires a value"))?;
                if flag == "--model" {
                    model = Some(value.clone());
                }
                if flag == "--setting-sources" {
                    setting_sources = Some(value.clone());
                }
                initial.push(value.clone());
                resumed.push(value.clone());
            } else if inline.is_none() && (optional || multiple) {
                while args.peek().is_some_and(|v| !v.starts_with('-')) {
                    if let Some(value) = args.next() {
                        initial.push(value.clone());
                        resumed.push(value.clone());
                    }
                    if optional {
                        break;
                    }
                }
            }
        }
        validate_auth_settings(&settings)
            .context("Claude launch settings conflict with managed profile authentication")?;
        validate_settings_sources(setting_sources.as_deref())?;
        let root = settings
            .as_object_mut()
            .context("Claude launch settings must be an object")?;
        // Since Claude v2.1.186, ! commands otherwise trigger a model response.
        // Keep local profile switching available even when the current account is exhausted.
        root.insert("respondToBashCommands".into(), Value::Bool(false));
        if root.get("disableAllHooks").and_then(Value::as_bool) == Some(true) {
            bail!("profile switching requires Claude hooks; disableAllHooks is enabled");
        }
        let hooks = root
            .entry("hooks")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .context("settings hooks must be an object")?;
        for event in ["SessionStart", "CwdChanged", "PostModelSwitch"] {
            let handlers = hooks
                .entry(event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .with_context(|| format!("{event} hooks must be an array"))?;
            handlers
                .push(json!({"hooks": [{"type":"command", "command":hook_command, "timeout":10}]}));
        }
        Ok(Self {
            initial,
            resumed,
            model,
            settings,
        })
    }

    pub fn write_settings(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec(&self.settings)?)
            .context("write scoped Claude launch settings")
    }

    pub fn command(
        &self,
        settings_path: &Path,
        resume: Option<&str>,
        model: Option<&str>,
        token: &str,
        label: &str,
        id: i64,
    ) -> Command {
        let mut command = Command::new(crate::resolve_claude_binary());
        command.args(if resume.is_some() {
            &self.resumed
        } else {
            &self.initial
        });
        if let Some(session_id) = resume {
            command.args(["--resume", session_id]);
            if let Some(model) = model {
                command.args(["--model", model]);
            }
        }
        command.arg("--settings").arg(settings_path);
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
            "CLAUDE_CODE_SIMPLE",
            "CLAUDE_CODE_SAFE_MODE",
            "MANDO_CREDENTIAL_ID",
            "MANDO_CREDENTIAL_LABEL",
        ] {
            command.env_remove(key);
        }
        command
            .env("CLAUDE_CODE_OAUTH_TOKEN", token)
            .env("MANDO_CREDENTIAL_ID", id.to_string())
            .env("MANDO_CREDENTIAL_LABEL", label);
        command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        command
    }
}

/// Quote an executable path for a hook's POSIX shell command.
pub fn hook_command(executable: &Path) -> Result<String> {
    let path = executable
        .to_str()
        .context("mando executable path is not UTF-8")?;
    Ok(format!("'{}' claude-hook", path.replace('\'', "'\\''")))
}

pub fn hook_settings_path(directory: &Path) -> PathBuf {
    directory.join("settings.json")
}

fn validate_auth_settings(settings: &Value) -> Result<()> {
    if settings
        .get("apiKeyHelper")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
    {
        bail!("apiKeyHelper is configured; remove it from active settings before using a managed OAuth profile");
    }
    if settings.get("disableAllHooks").and_then(Value::as_bool) == Some(true) {
        bail!("disableAllHooks prevents exact-session tracking");
    }
    if let Some(env) = settings.get("env") {
        let env = env
            .as_object()
            .context("Claude settings env must be an object")?;
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
            "CLAUDE_CODE_SIMPLE",
            "CLAUDE_CODE_SAFE_MODE",
        ] {
            if env.get(key).is_some_and(|value| {
                value
                    .as_str()
                    .is_none_or(|text| !text.is_empty() && text != "0" && text != "false")
            }) {
                bail!("settings env defines {key}; remove this conflicting authentication override first");
            }
        }
    }
    Ok(())
}

fn validate_settings_sources(sources: Option<&str>) -> Result<()> {
    let sources: Vec<&str> = sources.unwrap_or("user,project,local").split(',').collect();
    let mut files = vec![PathBuf::from(
        "/Library/Application Support/ClaudeCode/managed-settings.json",
    )];
    if sources.contains(&"user") {
        let config = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")));
        if let Some(config) = config {
            files.push(config.join("settings.json"));
        }
    }
    let cwd = std::env::current_dir()?;
    for directory in cwd.ancestors() {
        if sources.contains(&"project") {
            files.push(directory.join(".claude/settings.json"));
        }
        if sources.contains(&"local") {
            files.push(directory.join(".claude/settings.local.json"));
        }
    }
    files.sort();
    files.dedup();
    for file in files {
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read Claude settings {}", file.display()))
            }
        };
        let settings: Value = serde_json::from_str(&text)
            .with_context(|| format!("parse Claude settings {}", file.display()))?;
        validate_auth_settings(&settings).with_context(|| {
            format!(
                "Claude settings {} conflict with managed launch",
                file.display()
            )
        })?;
    }
    Ok(())
}
