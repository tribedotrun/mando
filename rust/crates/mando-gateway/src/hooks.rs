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
    global_types::home_dir()
        .join(".claude")
        .join("hooks")
        .join("mando-session-notify.sh")
}

/// Path to Claude settings.json.
fn claude_settings_path() -> PathBuf {
    global_types::home_dir()
        .join(".claude")
        .join("settings.json")
}

// Dispatches on the first CLI arg: `session-start` posts the CC session id
// to /cc-session; `user-prompt` posts an activity touch to /activity.
const HOOK_SCRIPT: &str = r#"#!/bin/bash
# Mando session hook -- notifies daemon of CC session events.
# Managed by Mando -- do not edit manually.
LOG="${HOME}/.claude/hooks/mando-session-notify.log"
EVENT="${1:-session-start}"
[ -z "$MANDO_TERMINAL_ID" ] && exit 0
case "$EVENT" in
  session-start)
    if ! command -v jq >/dev/null 2>&1; then
      echo "$(date -u +%FT%TZ) jq not found on PATH" >>"$LOG"; exit 0
    fi
    PAYLOAD=$(cat)
    SESSION_ID=$(echo "$PAYLOAD" | jq -r '.session_id // empty')
    [ -z "$SESSION_ID" ] && exit 0
    # Skip when CC ran from a different cwd than the terminal's home (user
    # cd'd inside the shell and started a fresh `claude`). CC keys conversation
    # files by the cwd at startup, so capturing this session id under the
    # terminal's home cwd would break `claude --resume` after a daemon
    # restart with "No conversation found".
    #
    # Both sides are canonicalized via `realpath` before comparison so that
    # symlinked components (e.g. `/var` ↔ `/private/var` on macOS) don't
    # produce a false mismatch. Daemon-side: terminal::session canonicalizes
    # before exporting MANDO_TERMINAL_CWD. Hook-side: realpath here.
    if [ -n "$MANDO_TERMINAL_CWD" ]; then
      CC_CWD=$(echo "$PAYLOAD" | jq -r '.cwd // empty')
      if [ -z "$CC_CWD" ]; then
        # CC version old enough to omit `.cwd` from the SessionStart payload.
        # Log so operators can spot when the guard couldn't fire and decide
        # whether the upgrade is worth pushing; pass through to preserve the
        # legacy capture path.
        echo "$(date -u +%FT%TZ) cc-session payload missing .cwd for terminal ${MANDO_TERMINAL_ID} — guard cannot fire, falling back to capture" >>"$LOG"
      else
        # realpath is in coreutils on Linux and shipped with macOS. Fall back
        # to the raw path if it's missing or the path doesn't resolve.
        if command -v realpath >/dev/null 2>&1; then
          CC_CWD_CANON=$(realpath "$CC_CWD" 2>/dev/null || echo "$CC_CWD")
          TERM_CWD_CANON=$(realpath "$MANDO_TERMINAL_CWD" 2>/dev/null || echo "$MANDO_TERMINAL_CWD")
        else
          CC_CWD_CANON="$CC_CWD"
          TERM_CWD_CANON="$MANDO_TERMINAL_CWD"
        fi
        if [ "$CC_CWD_CANON" != "$TERM_CWD_CANON" ]; then
          echo "$(date -u +%FT%TZ) skipping cc-session capture for terminal ${MANDO_TERMINAL_ID} (cc_cwd=${CC_CWD_CANON} terminal_cwd=${TERM_CWD_CANON})" >>"$LOG"
          exit 0
        fi
      fi
    fi
    curl -sf "http://127.0.0.1:${MANDO_PORT}/api/terminal/${MANDO_TERMINAL_ID}/cc-session" \
      -H "Authorization: Bearer ${MANDO_AUTH_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "{\"ccSessionId\": \"${SESSION_ID}\"}" 2>>"$LOG" \
    || echo "$(date -u +%FT%TZ) curl failed for terminal ${MANDO_TERMINAL_ID} (session-start)" >>"$LOG"
    ;;
  user-prompt)
    curl -sf -X POST "http://127.0.0.1:${MANDO_PORT}/api/terminal/${MANDO_TERMINAL_ID}/activity" \
      -H "Authorization: Bearer ${MANDO_AUTH_TOKEN}" \
      -H "Content-Type: application/json" \
      -d '{}' 2>>"$LOG" \
    || echo "$(date -u +%FT%TZ) curl failed for terminal ${MANDO_TERMINAL_ID} (user-prompt)" >>"$LOG"
    ;;
  *)
    echo "$(date -u +%FT%TZ) unknown event $EVENT" >>"$LOG"
    ;;
esac
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

/// Replace any existing mando-owned hooks entries for `event_name` with a
/// single entry invoking `hook_path` with `arg`. Other (non-mando) entries
/// are preserved.
fn upsert_mando_hook_entry(
    hooks_obj: &mut serde_json::Map<String, serde_json::Value>,
    event_name: &str,
    hook_path: &str,
    arg: &str,
) -> anyhow::Result<()> {
    let entry = hooks_obj
        .entry(event_name.to_string())
        .or_insert_with(|| serde_json::json!([]));
    let arr = entry
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("{event_name} is not an array"))?;
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
                "command": format!("'{hook_path}' {arg}")
            }
        ]
    }));
    Ok(())
}

/// Ensure `~/.claude/settings.json` has exactly one SessionStart and one
/// UserPromptSubmit entry for Mando's session-notify script, pointing at
/// the current data dir. Any stale entries from other data dirs (dev,
/// sandbox, old prod paths) are removed so each event only fires one
/// callback to the current daemon.
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

    upsert_mando_hook_entry(hooks_obj, "SessionStart", &hook_str, "session-start")?;
    upsert_mando_hook_entry(hooks_obj, "UserPromptSubmit", &hook_str, "user-prompt")?;

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&root)?;
    let tmp_path = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &settings_path)?;

    Ok(())
}

// `setup_session_hooks` follows the test module; allow the clippy lint
// instead of moving the test block to the end of the file.
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::HOOK_SCRIPT;

    #[test]
    fn hook_script_skips_session_capture_on_cwd_mismatch() {
        let script = HOOK_SCRIPT;
        // The capture must happen before the curl POST and gate on the
        // CC-reported cwd matching the terminal's home cwd.
        assert!(
            script.contains("MANDO_TERMINAL_CWD"),
            "hook must consult MANDO_TERMINAL_CWD"
        );
        assert!(
            script.contains("CC_CWD") && script.contains(".cwd // empty"),
            "hook must read cwd from the CC payload"
        );
        let guard = script
            .find(r#""$CC_CWD_CANON" != "$TERM_CWD_CANON""#)
            .expect("hook must compare canonicalized payload cwd to terminal cwd");
        let curl = script
            .find("api/terminal/${MANDO_TERMINAL_ID}/cc-session")
            .expect("hook must POST to /cc-session");
        assert!(
            guard < curl,
            "cwd guard must run before the cc-session POST"
        );
    }

    #[test]
    fn hook_script_canonicalizes_both_paths() {
        // Symlink resolution avoids false mismatches like /var vs /private/var
        // on macOS. Both sides must run through realpath before comparison.
        let script = HOOK_SCRIPT;
        assert!(
            script.contains(r#"CC_CWD_CANON=$(realpath "$CC_CWD""#),
            "hook must canonicalize CC's reported cwd via realpath"
        );
        assert!(
            script.contains(r#"TERM_CWD_CANON=$(realpath "$MANDO_TERMINAL_CWD""#),
            "hook must canonicalize MANDO_TERMINAL_CWD via realpath"
        );
    }

    #[test]
    fn hook_script_logs_when_cc_payload_omits_cwd() {
        // Older CC builds omit `.cwd` from the SessionStart payload. Pass
        // through (legacy capture path) but log so the operator can see the
        // guard couldn't fire — silent fallback would mask a regression.
        let script = HOOK_SCRIPT;
        assert!(
            script.contains("cc-session payload missing .cwd"),
            "hook must log when CC omits cwd"
        );
    }
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
