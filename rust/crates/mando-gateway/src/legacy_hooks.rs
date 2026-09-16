//! Removal of the Claude Code session-notify hook Mando used to install.
//!
//! Until the in-app terminal was removed, the daemon wrote
//! `~/.claude/hooks/mando-session-notify.sh` on every start and kept
//! `SessionStart` / `UserPromptSubmit` entries pointing at it in
//! `~/.claude/settings.json`. That writer is gone, and with it the only code
//! that could ever take the entries back out — so every machine that ran an
//! older build still fires a curl at a `/api/terminal/...` route the daemon no
//! longer serves, on every Claude Code session start and every prompt.
//!
//! This is the matching uninstall: an idempotent prune at daemon startup that
//! removes only Mando's own entries and only Mando's own script, then does
//! nothing on every subsequent start.

use std::path::{Path, PathBuf};

use tracing::{info, warn};

/// Substring identifying a hook command that invokes Mando's script. Matches
/// the filename rather than a full path so entries written by an older build
/// under a different data dir are cleaned up too.
const HOOK_SCRIPT_NAME: &str = "mando-session-notify.sh";

/// Header line every generated copy of the script carried. The script is
/// deleted only when this is present, so a file a human wrote at that path is
/// left alone.
const MANAGED_MARKER: &str = "Managed by Mando";

/// Claude Code hook events Mando registered under.
const MANAGED_EVENTS: &[&str] = &["SessionStart", "UserPromptSubmit"];

fn claude_dir() -> PathBuf {
    global_infra::paths::home_dir().join(".claude")
}

/// Run at daemon startup. Every step is best-effort: a machine with an
/// unreadable or hand-edited `settings.json` still boots, it just keeps the
/// stale entries and says so in the log.
pub fn prune_legacy_session_hooks() {
    let claude_dir = claude_dir();
    let settings_path = claude_dir.join("settings.json");
    match prune_settings_file(&settings_path) {
        Ok(0) => {}
        Ok(removed) => info!(
            module = "legacy-hooks",
            removed,
            path = %settings_path.display(),
            "removed stale Mando session-notify hook entries from Claude settings"
        ),
        Err(e) => warn!(
            module = "legacy-hooks",
            path = %settings_path.display(),
            error = %e,
            "could not prune stale Mando session-notify hook entries"
        ),
    }

    let script_path = claude_dir.join("hooks").join(HOOK_SCRIPT_NAME);
    match remove_managed_script(&script_path) {
        Ok(true) => info!(
            module = "legacy-hooks",
            path = %script_path.display(),
            "removed the stale Mando session-notify hook script"
        ),
        Ok(false) => {}
        Err(e) => warn!(
            module = "legacy-hooks",
            path = %script_path.display(),
            error = %e,
            "could not remove the stale Mando session-notify hook script"
        ),
    }
}

/// Strip Mando's hook entries from `settings.json`, returning how many were
/// removed. A missing file is zero, not an error. Everything else in the file
/// — other hooks, unrelated top-level keys, key order — round-trips through
/// serde untouched.
fn prune_settings_file(settings_path: &Path) -> anyhow::Result<usize> {
    if !settings_path.exists() {
        return Ok(0);
    }
    let contents = std::fs::read_to_string(settings_path)?;
    let mut root: serde_json::Value = serde_json::from_str(&contents)?;
    let removed = prune_settings_value(&mut root)?;
    if removed == 0 {
        return Ok(0);
    }

    let json = serde_json::to_string_pretty(&root)?;
    let tmp_path = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, format!("{json}\n"))?;
    std::fs::rename(&tmp_path, settings_path)?;
    Ok(removed)
}

/// The pure half: mutate a parsed settings document in place.
fn prune_settings_value(root: &mut serde_json::Value) -> anyhow::Result<usize> {
    let Some(obj) = root.as_object_mut() else {
        anyhow::bail!("settings.json root is not an object");
    };
    let Some(hooks) = obj.get_mut("hooks") else {
        return Ok(0);
    };
    let Some(hooks_obj) = hooks.as_object_mut() else {
        anyhow::bail!("settings.json hooks is not an object");
    };

    let mut removed = 0;
    for event in MANAGED_EVENTS {
        let Some(entries) = hooks_obj.get_mut(*event).and_then(|e| e.as_array_mut()) else {
            continue;
        };
        let before = entries.len();
        entries.retain(|entry| !is_mando_hook_entry(entry));
        removed += before - entries.len();
        // Drop an event key this prune emptied; leaving `"SessionStart": []`
        // behind would be residue of the same install.
        if entries.is_empty() && before > 0 {
            hooks_obj.remove(*event);
        }
    }
    if removed > 0 && hooks_obj.is_empty() {
        obj.remove("hooks");
    }
    Ok(removed)
}

/// Whether one `hooks.<Event>[]` entry invokes Mando's session-notify script.
fn is_mando_hook_entry(entry: &serde_json::Value) -> bool {
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .is_some_and(|inner| {
            inner.iter().any(|hook| {
                hook.get("command")
                    .and_then(|c| c.as_str())
                    .is_some_and(|command| command.contains(HOOK_SCRIPT_NAME))
            })
        })
}

/// Delete the hook script, but only a copy Mando generated. Returns whether a
/// file was removed.
fn remove_managed_script(script_path: &Path) -> anyhow::Result<bool> {
    if !script_path.exists() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(script_path)?;
    if !contents.contains(MANAGED_MARKER) {
        info!(
            module = "legacy-hooks",
            path = %script_path.display(),
            "hook script is not Mando-managed; leaving it in place"
        );
        return Ok(false);
    }
    std::fs::remove_file(script_path)?;
    Ok(true)
}
