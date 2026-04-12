//! CC session hook infrastructure -- script template + settings.json sync.

use std::path::{Path, PathBuf};

use tracing::{info, warn};

/// Stable path for the Mando session-notify hook script. Lives under
/// `~/.claude/hooks/` so all daemon modes (prod, dev, sandbox) share a
/// single script and a single `~/.claude/settings.json` entry. The script
/// reads `MANDO_PORT` and `MANDO_AUTH_TOKEN` from the process env at call
/// time, so it naturally routes to whichever daemon spawned the Claude
/// process, regardless of which daemon most recently wrote the script.
fn hook_script_path() -> PathBuf {
    mando_types::home_dir()
        .join(".claude")
        .join("hooks")
        .join("mando-session-notify.sh")
}

/// Path to Claude settings.json.
fn claude_settings_path() -> PathBuf {
    mando_types::home_dir()
        .join(".claude")
        .join("settings.json")
}

const HOOK_SCRIPT: &str = r#"#!/bin/bash
# Mando session hook -- notifies daemon of new CC sessions.
# Managed by Mando -- do not edit manually.
LOG="${HOME}/.claude/hooks/mando-session-notify.log"
[ -z "$MANDO_TERMINAL_ID" ] && exit 0
if ! command -v jq >/dev/null 2>&1; then
  echo "$(date -u +%FT%TZ) jq not found on PATH" >>"$LOG"; exit 0
fi
SESSION_ID=$(jq -r '.session_id // empty')
[ -z "$SESSION_ID" ] && exit 0
curl -sf "http://127.0.0.1:${MANDO_PORT}/api/terminal/${MANDO_TERMINAL_ID}/cc-session" \
  -H "Authorization: Bearer ${MANDO_AUTH_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"ccSessionId\": \"${SESSION_ID}\"}" 2>>"$LOG" \
|| echo "$(date -u +%FT%TZ) curl failed for terminal ${MANDO_TERMINAL_ID}" >>"$LOG"
"#;

/// Write the hook script template to disk (idempotent).
fn ensure_hook_script() -> anyhow::Result<PathBuf> {
    let path = hook_script_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, HOOK_SCRIPT)?;

    // Make executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(path)
}

/// Ensure `~/.claude/settings.json` has exactly one SessionStart entry for
/// Mando's session-notify script, pointing at the current data dir. Any
/// stale `session-notify.sh` entries from other data dirs (dev, sandbox,
/// old prod paths) are removed so each Claude session only fires one
/// callback.
fn sync_claude_settings(hook_path: &Path) -> anyhow::Result<()> {
    let settings_path = claude_settings_path();
    let hook_str = hook_path.to_string_lossy().to_string();

    let mut root: serde_json::Value = if settings_path.exists() {
        let contents = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&contents)?
    } else {
        serde_json::json!({})
    };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json root is not an object"))?;

    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;
    let session_start = hooks_obj
        .entry("SessionStart")
        .or_insert_with(|| serde_json::json!([]));
    let arr = session_start
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("SessionStart is not an array"))?;

    arr.retain(|entry| {
        let is_ours = entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|inner| {
                inner.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|s| s.contains("mando") && s.contains("session-notify"))
                })
            });
        !is_ours
    });

    arr.push(serde_json::json!({
        "hooks": [
            {
                "type": "command",
                "command": hook_str
            }
        ]
    }));

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&root)?;
    let tmp_path = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &settings_path)?;

    Ok(())
}

/// Run at daemon startup: write hook script and sync settings.json.
pub fn setup_session_hooks() {
    match ensure_hook_script() {
        Ok(path) => {
            info!(module = "hooks", path = %path.display(), "session hook script ready");
            if let Err(e) = sync_claude_settings(&path) {
                warn!(module = "hooks", error = %e, "failed to sync session hook to claude settings");
            }
        }
        Err(e) => {
            warn!(module = "hooks", error = %e, "failed to write session hook script");
        }
    }
}
