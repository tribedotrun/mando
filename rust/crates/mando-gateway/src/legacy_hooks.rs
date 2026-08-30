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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mando-legacy-hooks-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
    }

    fn read_json(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    const MANDO_SETTINGS: &str = r#"{
      "model": "opus",
      "hooks": {
        "SessionStart": [
          { "hooks": [ { "type": "command", "command": "'/Users/x/.claude/hooks/mando-session-notify.sh' session-start" } ] }
        ],
        "UserPromptSubmit": [
          { "hooks": [ { "type": "command", "command": "'/Users/x/.claude/hooks/mando-session-notify.sh' user-prompt" } ] }
        ]
      }
    }"#;

    #[test]
    fn removes_both_managed_hook_entries() {
        let dir = temp_dir();
        let settings = dir.join("settings.json");
        write(&settings, MANDO_SETTINGS);

        assert_eq!(prune_settings_file(&settings).unwrap(), 2);

        let root = read_json(&settings);
        assert_eq!(root.get("model").and_then(|v| v.as_str()), Some("opus"));
        assert!(
            root.get("hooks").is_none(),
            "an emptied hooks object is residue of the same install: {root}"
        );
    }

    #[test]
    fn a_second_run_is_a_no_op() {
        let dir = temp_dir();
        let settings = dir.join("settings.json");
        write(&settings, MANDO_SETTINGS);

        prune_settings_file(&settings).unwrap();
        let after_first = std::fs::read_to_string(&settings).unwrap();
        assert_eq!(prune_settings_file(&settings).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), after_first);
    }

    #[test]
    fn preserves_unrelated_hooks_on_the_same_event() {
        let dir = temp_dir();
        let settings = dir.join("settings.json");
        write(
            &settings,
            r#"{
              "hooks": {
                "SessionStart": [
                  { "hooks": [ { "type": "command", "command": "~/.claude/hooks/mando-session-notify.sh session-start" } ] },
                  { "hooks": [ { "type": "command", "command": "my-own-hook.sh" } ] }
                ],
                "Stop": [
                  { "hooks": [ { "type": "command", "command": "say done" } ] }
                ]
              }
            }"#,
        );

        assert_eq!(prune_settings_file(&settings).unwrap(), 1);

        let root = read_json(&settings);
        let session_start = root
            .pointer("/hooks/SessionStart")
            .and_then(|v| v.as_array())
            .expect("SessionStart survives with its remaining entry");
        assert_eq!(session_start.len(), 1);
        assert_eq!(
            session_start[0].pointer("/hooks/0/command").unwrap(),
            "my-own-hook.sh"
        );
        assert!(root.pointer("/hooks/Stop").is_some(), "{root}");
    }

    #[test]
    fn a_missing_settings_file_is_a_no_op() {
        let dir = temp_dir();
        assert_eq!(
            prune_settings_file(&dir.join("settings.json")).unwrap(),
            0,
            "a machine that never ran Claude Code has nothing to prune"
        );
    }

    #[test]
    fn settings_without_hooks_are_untouched() {
        let dir = temp_dir();
        let settings = dir.join("settings.json");
        write(&settings, r#"{"model":"opus"}"#);

        assert_eq!(prune_settings_file(&settings).unwrap(), 0);
        assert_eq!(
            std::fs::read_to_string(&settings).unwrap(),
            r#"{"model":"opus"}"#
        );
    }

    #[test]
    fn removes_the_managed_script() {
        let dir = temp_dir();
        let script = dir.join("mando-session-notify.sh");
        write(&script, "#!/bin/bash\n# Managed by Mando -- do not edit\n");

        assert!(remove_managed_script(&script).unwrap());
        assert!(!script.exists());
    }

    #[test]
    fn leaves_a_script_without_the_marker_alone() {
        let dir = temp_dir();
        let script = dir.join("mando-session-notify.sh");
        write(&script, "#!/bin/bash\necho hand written\n");

        assert!(!remove_managed_script(&script).unwrap());
        assert!(script.exists());
    }

    #[test]
    fn a_missing_script_is_a_no_op() {
        let dir = temp_dir();
        assert!(!remove_managed_script(&dir.join("mando-session-notify.sh")).unwrap());
    }
}
