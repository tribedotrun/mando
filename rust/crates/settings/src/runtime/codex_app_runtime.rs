//! Daemon-side orchestration for the ChatGPT desktop app's shared Codex
//! credential slot.
//!
//! `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`) is the single shared
//! slot the desktop app reads. A swap resolves a stored credential, quits
//! the app, preserves or syncs the outgoing slot, atomically writes the new
//! content, persists recovery state, and relaunches the app. Both Electron
//! and the CLI reach this service through typed daemon routes.
//!
//! CREDENTIAL-SENSITIVE: this service only picks and syncs credential rows.
//! It never deletes or disables a credential and never runs `codex logout`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{CodexCredentialError, SettingsRuntime};
use crate::io::codex_app_process;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SwapMode {
    #[default]
    Ambient,
    Pool,
}

/// Credential-sensitive local state persisted in Mando's state directory.
/// The file is created with mode 0600 on Unix because `stash_auth_json`
/// contains the personal account's raw token material.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SwapState {
    mode: SwapMode,
    label: Option<String>,
    credential_id: Option<i64>,
    /// Account id written into the slot by the last successful swap. Used
    /// to detect an out-of-band account change before syncing a pool row.
    account_id: Option<String>,
    /// Personal/ambient slot content retained for a later restore.
    stash_auth_json: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexDesktopAppError {
    #[error("no usable Codex credential for label {0:?}")]
    NoUsableCredential(String),
    #[error("no pool credential is checked out (ChatGPT desktop app is not swapped)")]
    NotSwapped,
    #[error("no stashed personal account to restore")]
    NoPersonalStash,
    #[error(transparent)]
    Credential(#[from] CodexCredentialError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
struct CodexAppPaths {
    slot: PathBuf,
    state_dir: PathBuf,
}

impl CodexAppPaths {
    fn resolve(codex_home: Option<&Path>) -> Self {
        Self {
            slot: codex_home_dir(codex_home).join("auth.json"),
            state_dir: global_infra::paths::state_dir(),
        }
    }

    fn state(&self) -> PathBuf {
        self.state_dir.join("codex-app-swap.json")
    }

    fn recovery(&self, credential_id: i64) -> PathBuf {
        self.state_dir
            .join(format!("codex-app-swap-recovery-{credential_id}.json"))
    }
}

impl SettingsRuntime {
    /// Swap the named pooled credential into the ChatGPT desktop app.
    #[tracing::instrument(skip_all, fields(label = label))]
    pub async fn use_codex_desktop_app(
        &self,
        label: &str,
        codex_home: Option<&Path>,
        caller_pid: Option<u32>,
        bus: &global_bus::EventBus,
    ) -> std::result::Result<api_types::CodexDesktopAppOperationResponse, CodexDesktopAppError>
    {
        guard_keychain_absent().await?;

        // Resolve and validate the target before stopping the app or touching
        // its slot, so a bad label leaves the desktop app undisturbed.
        let label = label.trim();
        let pick = self
            .pick_codex_credential_explicit(None, Some(label))
            .await?
            .ok_or_else(|| CodexDesktopAppError::NoUsableCredential(label.to_string()))?;
        if pick.auth_json.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "credential #{} ({}) produced an empty auth_json",
                pick.id,
                pick.label
            )
            .into());
        }

        let paths = CodexAppPaths::resolve(codex_home);
        let mut state = load_state(&paths)?;

        codex_app_process::quit_chatgpt_app().await?;
        let mut warnings = Vec::new();
        for warning in codex_app_process::external_codex_process_warnings(caller_pid).await {
            push_warning(&mut warnings, warning);
        }

        // Read only after the app has fully quit so any tokens flushed or
        // rotated during shutdown are the version checked back into Mando.
        let current_slot = read_slot_raw(&paths.slot)?;
        self.checkin_previous_occupant(
            &paths,
            &mut state,
            current_slot.as_deref(),
            &mut warnings,
            bus,
        )
        .await;

        // Persist the outgoing account before overwriting its slot so a
        // crash cannot lose the only on-disk copy of its credentials.
        save_state(&paths, &state)?;
        write_private_atomic(&paths.slot, &pick.auth_json)?;

        state.mode = SwapMode::Pool;
        state.label = Some(pick.label.clone());
        state.credential_id = Some(pick.id);
        state.account_id = Some(pick.account_id.clone());
        save_state(&paths, &state)?;

        if let Some(warning) = codex_app_process::relaunch_chatgpt_app().await {
            push_warning(&mut warnings, warning);
        }

        Ok(api_types::CodexDesktopAppOperationResponse {
            message: format!(
                "ChatGPT desktop app is now using pool account '{}' (#{}, {}).",
                pick.label, pick.id, pick.account_id
            ),
            warnings,
        })
    }

    /// Sync the checked-out pool account and restore the stashed personal
    /// account to the ChatGPT desktop app.
    #[tracing::instrument(skip_all)]
    pub async fn restore_codex_desktop_app(
        &self,
        codex_home: Option<&Path>,
        bus: &global_bus::EventBus,
    ) -> std::result::Result<api_types::CodexDesktopAppOperationResponse, CodexDesktopAppError>
    {
        guard_keychain_absent().await?;

        let paths = CodexAppPaths::resolve(codex_home);
        let mut state = load_state(&paths)?;
        if state.mode != SwapMode::Pool {
            return Err(CodexDesktopAppError::NotSwapped);
        }
        let stash = state
            .stash_auth_json
            .clone()
            .ok_or(CodexDesktopAppError::NoPersonalStash)?;
        let credential_id = state.credential_id;
        let label = state.label.clone();

        codex_app_process::quit_chatgpt_app().await?;

        let mut warnings = Vec::new();
        if let Some(credential_id) = credential_id {
            if let Some(current) = read_slot_raw(&paths.slot)? {
                match self.sync_codex_credential(credential_id, &current).await {
                    Ok(()) => bus.send(global_bus::BusPayload::Credentials(None)),
                    Err(error) => {
                        push_warning(
                            &mut warnings,
                            sync_failed_warning(&paths, credential_id, &current, &error),
                        );
                    }
                }
            }
        }

        write_private_atomic(&paths.slot, &stash)?;

        state.mode = SwapMode::Ambient;
        state.label = None;
        state.credential_id = None;
        state.account_id = slot_account_id(&stash);
        state.stash_auth_json = Some(stash);
        save_state(&paths, &state)?;

        if let Some(warning) = codex_app_process::relaunch_chatgpt_app().await {
            push_warning(&mut warnings, warning);
        }

        let label_note = label
            .map(|value| format!(" (was '{value}')"))
            .unwrap_or_default();
        Ok(api_types::CodexDesktopAppOperationResponse {
            message: format!(
                "ChatGPT desktop app restored to the personal/ambient account{label_note}."
            ),
            warnings,
        })
    }

    /// Report the account currently occupying the ChatGPT desktop app slot.
    #[tracing::instrument(skip_all)]
    pub fn codex_desktop_app_status(
        &self,
        codex_home: Option<&Path>,
    ) -> std::result::Result<api_types::CodexDesktopAppStatusResponse, CodexDesktopAppError> {
        status_at(&CodexAppPaths::resolve(codex_home)).map_err(Into::into)
    }

    async fn checkin_previous_occupant(
        &self,
        paths: &CodexAppPaths,
        state: &mut SwapState,
        current_slot: Option<&str>,
        warnings: &mut Vec<String>,
        bus: &global_bus::EventBus,
    ) {
        let Some(current) = current_slot else {
            return;
        };

        let is_tracked_pool_occupant = state.mode == SwapMode::Pool
            && state.account_id.is_some()
            && state.account_id.as_deref() == slot_account_id(current).as_deref();

        if is_tracked_pool_occupant {
            if let Some(credential_id) = state.credential_id {
                match self.sync_codex_credential(credential_id, current).await {
                    Ok(()) => {
                        bus.send(global_bus::BusPayload::Credentials(None));
                    }
                    Err(error) => {
                        push_warning(
                            warnings,
                            sync_failed_warning(paths, credential_id, current, &error),
                        );
                    }
                }
            }
        } else if state.mode != SwapMode::Pool {
            state.stash_auth_json = Some(current.to_string());
        } else {
            push_warning(
                warnings,
                "warning: slot account changed since checkout (app signed into a different account?); preserving the existing personal backup and not syncing the current occupant"
                    .to_string(),
            );
        }
    }
}

fn status_at(paths: &CodexAppPaths) -> Result<api_types::CodexDesktopAppStatusResponse> {
    let state = load_state(paths)?;
    let slot_raw = read_slot_best_effort(&paths.slot);
    let slot_account = slot_raw.as_deref().and_then(slot_account_id);

    // Saved pool state is authoritative only while the slot still contains
    // the account written by the service. An out-of-band sign-in is ambient.
    let slot_matches_tracked =
        state.account_id.is_some() && state.account_id.as_deref() == slot_account.as_deref();
    let mode = if slot_raw.is_none() {
        api_types::CodexDesktopAppMode::None
    } else if state.mode == SwapMode::Pool && slot_matches_tracked {
        api_types::CodexDesktopAppMode::Pool
    } else {
        api_types::CodexDesktopAppMode::Ambient
    };
    let is_pool = mode == api_types::CodexDesktopAppMode::Pool;

    Ok(api_types::CodexDesktopAppStatusResponse {
        mode,
        active_label: is_pool.then_some(state.label).flatten(),
        credential_id: is_pool.then_some(state.credential_id).flatten(),
        slot_account_id: slot_account,
        can_restore: state.stash_auth_json.is_some(),
    })
}

fn push_warning(warnings: &mut Vec<String>, warning: String) {
    tracing::warn!(
        module = "codex-desktop-app",
        warning,
        "Codex desktop app swap warning"
    );
    warnings.push(warning);
}

// ---------------------------------------------------------------------
// Keychain guard
// ---------------------------------------------------------------------

/// Refuse file swapping if the desktop app stores Codex auth in the macOS
/// Keychain. A successful lookup means `auth.json` is no longer authoritative.
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
// Credential-slot and private-state helpers
// ---------------------------------------------------------------------

fn codex_home_dir(override_path: Option<&Path>) -> PathBuf {
    if let Some(path) = override_path {
        return path.to_path_buf();
    }
    if let Ok(value) = std::env::var("CODEX_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    global_types::home_dir().join(".codex")
}

/// `Ok(None)` means the file does not exist. Other failures propagate so a
/// mutating operation never overwrites a slot it could not inspect.
fn read_slot_raw(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(anyhow::Error::from(error).context(format!("failed to read {}", path.display())))
        }
    }
}

/// Status is best-effort: missing or unreadable slot content reports `none`.
fn read_slot_best_effort(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Extract `tokens.account_id` from raw Codex auth JSON. Invalid or incomplete
/// content is an untracked slot rather than a fatal status/swap error.
fn slot_account_id(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("tokens")?
        .get("account_id")?
        .as_str()
        .map(str::to_string)
}

/// Write credential material atomically. On Unix the temporary file starts
/// at 0600, so tokens are never briefly exposed with broader permissions.
fn write_private_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("path has no file name")?;
    let temp = path.with_file_name(format!("{file_name}.tmp.{}", std::process::id()));
    match std::fs::remove_file(&temp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to remove stale temp file {}", temp.display()));
        }
    }

    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .with_context(|| format!("failed to create {}", temp.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync {}", temp.display()))?;
    }

    std::fs::rename(&temp, path)
        .with_context(|| format!("failed to rename {} to {}", temp.display(), path.display()))?;
    Ok(())
}

fn load_state(paths: &CodexAppPaths) -> Result<SwapState> {
    Ok(global_infra::load_json_file(&paths.state())?)
}

fn save_state(paths: &CodexAppPaths, state: &SwapState) -> Result<()> {
    let json = serde_json::to_string_pretty(state).context("failed to serialize swap state")?;
    write_private_atomic(&paths.state(), &json)
}

fn save_recovery_copy(paths: &CodexAppPaths, credential_id: i64, content: &str) -> Result<PathBuf> {
    let path = paths.recovery(credential_id);
    write_private_atomic(&path, content)?;
    Ok(path)
}

fn sync_failed_warning(
    paths: &CodexAppPaths,
    credential_id: i64,
    content: &str,
    error: &impl std::fmt::Display,
) -> String {
    match save_recovery_copy(paths, credential_id, content) {
        Ok(path) => format!(
            "warning: failed to sync outgoing pool credential #{credential_id}: {error}; its rotated tokens are preserved at {}",
            path.display()
        ),
        Err(save_error) => format!(
            "warning: failed to sync outgoing pool credential #{credential_id}: {error}; AND failed to write a recovery copy: {save_error} — its rotated tokens may be lost"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &Path) -> CodexAppPaths {
        CodexAppPaths {
            slot: root.join("codex/auth.json"),
            state_dir: root.join("state"),
        }
    }

    fn auth(account_id: &str) -> String {
        format!(
            r#"{{"auth_mode":"chatgpt","tokens":{{"access_token":"AT","account_id":"{account_id}"}}}}"#
        )
    }

    #[test]
    fn status_reports_pool_only_when_slot_matches_saved_account() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(temp.path());
        let slot = auth("acct-pool");
        write_private_atomic(&paths.slot, &slot).expect("write slot");
        save_state(
            &paths,
            &SwapState {
                mode: SwapMode::Pool,
                label: Some("work".into()),
                credential_id: Some(7),
                account_id: Some("acct-pool".into()),
                stash_auth_json: Some(auth("acct-personal")),
            },
        )
        .expect("save state");

        let status = status_at(&paths).expect("status");
        assert_eq!(status.mode, api_types::CodexDesktopAppMode::Pool);
        assert_eq!(status.active_label.as_deref(), Some("work"));
        assert_eq!(status.credential_id, Some(7));
        assert_eq!(status.slot_account_id.as_deref(), Some("acct-pool"));
        assert!(status.can_restore);
    }

    #[test]
    fn status_treats_out_of_band_account_change_as_ambient() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(temp.path());
        write_private_atomic(&paths.slot, &auth("acct-other")).expect("write slot");
        save_state(
            &paths,
            &SwapState {
                mode: SwapMode::Pool,
                label: Some("work".into()),
                credential_id: Some(7),
                account_id: Some("acct-pool".into()),
                stash_auth_json: Some(auth("acct-personal")),
            },
        )
        .expect("save state");

        let status = status_at(&paths).expect("status");
        assert_eq!(status.mode, api_types::CodexDesktopAppMode::Ambient);
        assert!(status.active_label.is_none());
        assert!(status.credential_id.is_none());
        assert_eq!(status.slot_account_id.as_deref(), Some("acct-other"));
        assert!(status.can_restore);
    }

    #[test]
    fn status_reports_none_for_a_missing_slot_but_retains_restore_capability() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(temp.path());
        save_state(
            &paths,
            &SwapState {
                stash_auth_json: Some(auth("acct-personal")),
                ..SwapState::default()
            },
        )
        .expect("save state");

        let status = status_at(&paths).expect("status");
        assert_eq!(status.mode, api_types::CodexDesktopAppMode::None);
        assert!(status.active_label.is_none());
        assert!(status.credential_id.is_none());
        assert!(status.slot_account_id.is_none());
        assert!(status.can_restore);
    }

    #[test]
    fn slot_account_id_extracts_nested_field() {
        assert_eq!(slot_account_id(&auth("acct-1")).as_deref(), Some("acct-1"));
    }

    #[test]
    fn slot_account_id_rejects_missing_or_invalid_content() {
        assert!(slot_account_id(r#"{"tokens":{"access_token":"AT"}}"#).is_none());
        assert!(slot_account_id("not json").is_none());
    }

    #[test]
    fn write_private_atomic_round_trips_with_private_permissions() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("auth.json");
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
    }
}
