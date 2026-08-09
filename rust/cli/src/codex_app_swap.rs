//! Orchestration for `mando codex app-use` / `app-restore` / `app-status`.
//!
//! `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`) is the single shared
//! "slot" the ChatGPT desktop app reads from. Swapping a pooled account in
//! means: resolve the target credential, quit the app, back up whatever is
//! currently in the slot (sync it back to its pool row if it's a pool
//! credential we recognize, otherwise stash its raw bytes), write the new
//! content, persist local swap state, and relaunch.
//!
//! CREDENTIAL-SENSITIVE: this module only ever calls the credential *pick*
//! and *sync* daemon routes. It must never call a delete/disable route, and
//! it must never run `codex logout`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::codex_app_process;
use crate::gateway_paths as paths;
use crate::http::DaemonClient;

/// Which account currently — as far as our last successful `app-use` /
/// `app-restore` call knows — occupies the shared `auth.json` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SwapMode {
    #[default]
    Ambient,
    Pool,
}

/// Local (not wire) state persisted at
/// `~/.mando/state/codex-app-swap.json` across `app-use` / `app-restore`
/// calls. Holds raw token material in `stash_auth_json` — the file is
/// chmod 0600 after every save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SwapState {
    pub mode: SwapMode,
    pub label: Option<String>,
    pub credential_id: Option<i64>,
    /// `account_id` currently written into the slot as of the last
    /// successful swap — used to detect whether the slot was changed by
    /// something else since then.
    pub account_id: Option<String>,
    /// Raw `auth.json` content of the personal/ambient account, stashed so
    /// `app-restore` can put it back.
    pub stash_auth_json: Option<String>,
}

/// `mando codex app-status --json` output. Hard contract parsed by the
/// Electron layer — field set and casing must not change without updating
/// that consumer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatusJson {
    mode: &'static str,
    active_label: Option<String>,
    credential_id: Option<i64>,
    slot_account_id: Option<String>,
    can_restore: bool,
}

// ---------------------------------------------------------------------
// app-use
// ---------------------------------------------------------------------

pub(crate) async fn handle_app_use(label: String) -> Result<()> {
    guard_keychain_absent().await?;

    // Resolve the target FIRST — before quitting anything or touching the
    // slot — so a bad label fails fast with the app still running.
    let client = DaemonClient::discover()?;
    let request = api_types::CredentialPickRequest {
        id: None,
        label: Some(label.clone()),
    };
    let response: api_types::CodexCredentialPickResponse = client
        .post_json(paths::CREDENTIALS_CODEX_PICK, &request)
        .await?;
    let pick = response
        .pick
        .ok_or_else(|| anyhow::anyhow!("no usable Codex credential for label {label:?}"))?;

    if pick.auth_json.trim().is_empty() {
        anyhow::bail!(
            "daemon returned an empty auth_json for credential #{} ({})",
            pick.id,
            pick.label
        );
    }

    let slot = slot_path();
    let mut state = load_state()?;

    codex_app_process::quit_chatgpt_app().await?;
    codex_app_process::warn_external_codex_processes().await;

    // Read the slot only AFTER the app has fully quit, so we capture any
    // tokens ChatGPT flushed or rotated during shutdown instead of a stale
    // pre-quit snapshot (which would lose the freshest tokens on check-in).
    let current_slot = read_slot_raw(&slot)?;

    // Non-destructive check-in of whoever currently occupies the slot,
    // before we overwrite it.
    checkin_previous_occupant(&client, &mut state, current_slot.as_deref()).await;

    // Persist the stash/backup to disk BEFORE overwriting the slot, so a
    // crash mid-swap can never lose the outgoing account's only on-disk copy.
    save_state(&state)?;

    write_private_atomic(&slot, &pick.auth_json)?;

    state.mode = SwapMode::Pool;
    state.label = Some(pick.label.clone());
    state.credential_id = Some(pick.id);
    state.account_id = Some(pick.account_id.clone());
    save_state(&state)?;

    codex_app_process::relaunch_chatgpt_app().await;

    println!(
        "ChatGPT desktop app is now using pool account '{}' (#{}, {}).",
        pick.label, pick.id, pick.account_id
    );
    Ok(())
}

/// Back up whatever currently sits in the slot before it gets overwritten.
///
/// If the state file says a pool credential is checked out AND the slot's
/// account_id still matches what we last wrote, sync its (possibly
/// rotated) tokens back to that credential's row. Otherwise — a personal
/// account, an untracked pool account, or a mismatch — stash the raw bytes
/// for `app-restore`. Sync failures are warnings, never fatal: losing the
/// ability to check in a rotation is recoverable, silently overwriting an
/// unbacked-up slot is not.
async fn checkin_previous_occupant(
    client: &DaemonClient,
    state: &mut SwapState,
    current_slot: Option<&str>,
) {
    let Some(current) = current_slot else {
        return;
    };

    let is_tracked_pool_occupant = state.mode == SwapMode::Pool
        && state.account_id.is_some()
        && state.account_id.as_deref() == slot_account_id(current).as_deref();

    if is_tracked_pool_occupant {
        if let Some(credential_id) = state.credential_id {
            let request = api_types::SyncCodexCredentialRequest {
                credential_id,
                auth_json: current.to_string(),
            };
            if let Err(err) = client
                .post_json::<api_types::SyncCodexCredentialResponse, _>(
                    paths::CREDENTIALS_CODEX_SYNC,
                    &request,
                )
                .await
            {
                warn_sync_failed_with_recovery(credential_id, current, &err);
            }
        }
    } else if state.mode != SwapMode::Pool {
        // Genuine personal/ambient occupant — stash it so `app-restore`
        // can put it back.
        state.stash_auth_json = Some(current.to_string());
    } else {
        // We recorded a pool checkout, but the slot's account no longer
        // matches it — the app was signed into a different account outside
        // this tool. Don't sync an unknown row, and don't clobber the real
        // personal backup; just warn.
        eprintln!(
            "warning: slot account changed since checkout (app signed into a different account?); preserving the existing personal backup and not syncing the current occupant"
        );
    }
}

// ---------------------------------------------------------------------
// app-restore
// ---------------------------------------------------------------------

pub(crate) async fn handle_app_restore() -> Result<()> {
    guard_keychain_absent().await?;

    let mut state = load_state()?;
    if state.mode != SwapMode::Pool {
        anyhow::bail!("no pool credential is checked out (ChatGPT desktop app is not swapped)");
    }
    let Some(stash) = state.stash_auth_json.clone() else {
        anyhow::bail!("no stashed personal account to restore");
    };
    let credential_id = state.credential_id;
    let label = state.label.clone();

    codex_app_process::quit_chatgpt_app().await?;

    let slot = slot_path();
    if let Some(credential_id) = credential_id {
        if let Some(current) = read_slot_raw(&slot)? {
            let client = DaemonClient::discover()?;
            let request = api_types::SyncCodexCredentialRequest {
                credential_id,
                auth_json: current.clone(),
            };
            if let Err(err) = client
                .post_json::<api_types::SyncCodexCredentialResponse, _>(
                    paths::CREDENTIALS_CODEX_SYNC,
                    &request,
                )
                .await
            {
                warn_sync_failed_with_recovery(credential_id, &current, &err);
            }
        }
    }

    write_private_atomic(&slot, &stash)?;

    state.mode = SwapMode::Ambient;
    state.label = None;
    state.credential_id = None;
    state.account_id = slot_account_id(&stash);
    state.stash_auth_json = Some(stash);
    save_state(&state)?;

    codex_app_process::relaunch_chatgpt_app().await;

    let label_note = label.map(|l| format!(" (was '{l}')")).unwrap_or_default();
    println!("ChatGPT desktop app restored to the personal/ambient account{label_note}.");
    Ok(())
}

// ---------------------------------------------------------------------
// app-status
// ---------------------------------------------------------------------

pub(crate) async fn handle_app_status(as_json: bool) -> Result<()> {
    let state = load_state()?;
    let slot = slot_path();
    let slot_raw = read_slot_best_effort(&slot);
    let slot_account = slot_raw.as_deref().and_then(slot_account_id);

    // Only report "pool" if the slot still actually holds the account we
    // checked out. If the desktop app was signed into a different account
    // outside this tool, the saved pool state is stale — report "ambient" so
    // the UI banner doesn't keep claiming the old label is active.
    let slot_matches_tracked =
        state.account_id.is_some() && state.account_id.as_deref() == slot_account.as_deref();
    let mode: &'static str = if slot_raw.is_none() {
        "none"
    } else if state.mode == SwapMode::Pool && slot_matches_tracked {
        "pool"
    } else {
        "ambient"
    };
    let can_restore = state.stash_auth_json.is_some();
    let active_label = if mode == "pool" {
        state.label.clone()
    } else {
        None
    };
    let credential_id = if mode == "pool" {
        state.credential_id
    } else {
        None
    };

    if as_json {
        let payload = AppStatusJson {
            mode,
            active_label,
            credential_id,
            slot_account_id: slot_account,
            can_restore,
        };
        println!("{}", serde_json::to_string(&payload)?);
        return Ok(());
    }

    match mode {
        "pool" => {
            let label = active_label.as_deref().unwrap_or("?");
            match credential_id {
                Some(id) => println!("ChatGPT desktop app: using pool account '{label}' (#{id})"),
                None => println!("ChatGPT desktop app: using pool account '{label}'"),
            }
        }
        "ambient" => println!("ChatGPT desktop app: using personal/ambient account"),
        _ => println!("ChatGPT desktop app: unknown (no ~/.codex/auth.json)"),
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Keychain guard
// ---------------------------------------------------------------------

/// Bail if the ChatGPT desktop app now stores Codex auth in the macOS
/// Keychain (a "Codex Auth" generic-password item exists) — on that
/// install, swapping `~/.codex/auth.json` on disk is unsafe because the
/// app never reads it. Any other outcome (item not found, or the
/// `security` lookup itself failing) is treated as safe to proceed.
#[cfg(target_os = "macos")]
async fn guard_keychain_absent() -> Result<()> {
    let output = tokio::process::Command::new("security")
        .args(["find-generic-password", "-s", "Codex Auth"])
        .output()
        .await
        .context("failed to run `security find-generic-password`")?;
    if output.status.success() {
        anyhow::bail!(
            "ChatGPT desktop now stores Codex auth in the macOS Keychain (found a \"Codex Auth\" item) — file-based ~/.codex/auth.json swapping is unsafe on this install; not touching the slot"
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
async fn guard_keychain_absent() -> Result<()> {
    anyhow::bail!("mando codex app-use/app-restore are macOS-only (ChatGPT desktop app)")
}

// ---------------------------------------------------------------------
// Slot (~/.codex/auth.json) helpers
// ---------------------------------------------------------------------

fn codex_home_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CODEX_HOME") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    global_types::home_dir().join(".codex")
}

fn slot_path() -> PathBuf {
    codex_home_dir().join("auth.json")
}

/// Read the slot's raw bytes. `Ok(None)` means the file doesn't exist yet;
/// any other read failure (permission denied, etc.) is propagated since a
/// mutating caller must not blindly overwrite a slot it couldn't verify.
fn read_slot_raw(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::from(e).context(format!("failed to read {}", path.display()))),
    }
}

/// Read the slot's raw bytes for display purposes only. Any failure
/// (missing, unreadable, permission denied) collapses to `None` — status
/// reporting must never error out over a file it can't see.
fn read_slot_best_effort(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Extract `tokens.account_id` from a raw Codex `auth.json` blob.
/// `None` on any parse failure or missing field — non-fatal by design.
fn slot_account_id(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("tokens")?
        .get("account_id")?
        .as_str()
        .map(str::to_string)
}

/// Write `content` to `path` atomically (temp file + rename). On unix the
/// temp file is created with mode 0600 from the start, so token material is
/// never briefly world-readable through the temp file. Used for every file
/// this module writes that can hold raw tokens: the slot, the local state
/// file, and recovery copies.
fn write_private_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("path has no file name")?;
    let tmp = path.with_file_name(format!("{file_name}.tmp.{}", std::process::id()));
    // Clear any stale temp so the create_new below always makes a fresh
    // 0600 file rather than reusing one with looser permissions.
    let _ = std::fs::remove_file(&tmp);

    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("failed to create {}", tmp.display()))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        f.sync_all()
            .with_context(|| format!("failed to fsync {}", tmp.display()))?;
    }

    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------
// Local state file (~/.mando/state/codex-app-swap.json)
// ---------------------------------------------------------------------

fn state_path() -> PathBuf {
    global_infra::paths::state_dir().join("codex-app-swap.json")
}

fn load_state() -> Result<SwapState> {
    Ok(global_infra::load_json_file(
        &state_path(),
        "codex_app_swap",
    )?)
}

fn save_state(state: &SwapState) -> Result<()> {
    // Serialize and write through the private-atomic writer so the state
    // temp file (which holds the stashed personal account's tokens) is 0600
    // from creation, not merely chmodded after the token bytes are written.
    let json = serde_json::to_string_pretty(state).context("failed to serialize swap state")?;
    write_private_atomic(&state_path(), &json)
}

/// Path for a per-credential recovery copy of rotated tokens that could not
/// be synced back to the pool. Kept under the state dir at 0600.
fn recovery_path(credential_id: i64) -> PathBuf {
    global_infra::paths::state_dir().join(format!("codex-app-swap-recovery-{credential_id}.json"))
}

/// Persist the outgoing account's rotated `auth.json` to a private recovery
/// file so a failed sync cannot make the credential unrecoverable. Returns
/// the path written.
fn save_recovery_copy(credential_id: i64, content: &str) -> Result<PathBuf> {
    let path = recovery_path(credential_id);
    write_private_atomic(&path, content)?;
    Ok(path)
}

/// Report a failed outgoing-credential sync, preserving the rotated tokens to
/// a recovery file first so they are never the slot's only copy at the moment
/// it gets overwritten.
fn warn_sync_failed_with_recovery<E: std::fmt::Display>(
    credential_id: i64,
    content: &str,
    err: &E,
) {
    match save_recovery_copy(credential_id, content) {
        Ok(path) => eprintln!(
            "warning: failed to sync outgoing pool credential #{credential_id}: {err}; its rotated tokens are preserved at {}",
            path.display()
        ),
        Err(save_err) => eprintln!(
            "warning: failed to sync outgoing pool credential #{credential_id}: {err}; AND failed to write a recovery copy: {save_err} — its rotated tokens may be lost"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_account_id_extracts_nested_field() {
        let raw = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"AT","account_id":"acct-1"}}"#;
        assert_eq!(slot_account_id(raw).as_deref(), Some("acct-1"));
    }

    #[test]
    fn slot_account_id_missing_field_is_none() {
        let raw = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"AT"}}"#;
        assert!(slot_account_id(raw).is_none());
    }

    #[test]
    fn slot_account_id_invalid_json_is_none() {
        assert!(slot_account_id("not json").is_none());
    }

    #[test]
    fn swap_mode_default_is_ambient() {
        assert_eq!(SwapMode::default(), SwapMode::Ambient);
    }

    #[test]
    fn swap_state_default_has_no_stash() {
        let state = SwapState::default();
        assert_eq!(state.mode, SwapMode::Ambient);
        assert!(state.stash_auth_json.is_none());
        assert!(state.credential_id.is_none());
    }

    #[test]
    fn write_private_atomic_round_trip_and_permissions() {
        let dir = std::env::temp_dir().join(format!(
            "mando-codex-app-swap-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path = dir.join("auth.json");
        write_private_atomic(&path, "hello").expect("write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "hello");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_slot_raw_missing_file_is_none() {
        let path = std::env::temp_dir().join("mando-codex-app-swap-missing-slot.json");
        let _ = std::fs::remove_file(&path);
        assert!(read_slot_raw(&path)
            .expect("missing should be Ok")
            .is_none());
    }

    #[test]
    fn read_slot_best_effort_missing_file_is_none() {
        let path = std::env::temp_dir().join("mando-codex-app-swap-missing-slot-2.json");
        let _ = std::fs::remove_file(&path);
        assert!(read_slot_best_effort(&path).is_none());
    }
}
