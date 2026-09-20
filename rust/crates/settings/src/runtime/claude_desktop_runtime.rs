//! Persistent Desktop login profiles share Code files and retain independent login state.
use super::SettingsRuntime;
use crate::io::claude_desktop_profile::{self as profile, ProfileRegistration};
use anyhow::{Context, Result};
use api_types::{
    ClaudeDesktopProfileState, ClaudeDesktopProfileStatus, ClaudeDesktopSessionSyncResponse,
};
use std::path::{Path, PathBuf};

static PROFILE_OPERATIONS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, thiserror::Error)]
pub enum ClaudeDesktopProfileError {
    #[error("Claude credential #{0} does not exist")]
    NotFound(i64),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

type DesktopResult<T> = std::result::Result<T, ClaudeDesktopProfileError>;

impl SettingsRuntime {
    async fn validate_claude_profile_credential(&self, id: i64) -> DesktopResult<()> {
        let row = self
            .get_credential_row(id)
            .await
            .map_err(anyhow::Error::from)?;
        if row.is_none_or(|row| row.provider != "claude") {
            return Err(ClaudeDesktopProfileError::NotFound(id));
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(credential_id = id))]
    pub async fn claude_desktop_profile_status(
        &self,
        id: i64,
    ) -> DesktopResult<ClaudeDesktopProfileStatus> {
        self.validate_claude_profile_credential(id).await?;
        status(id).await.map_err(Into::into)
    }

    /// Set up a stable directory once; repeated calls preserve the existing registration.
    #[tracing::instrument(skip_all, fields(credential_id = id))]
    pub async fn setup_claude_desktop_profile(
        &self,
        id: i64,
    ) -> DesktopResult<ClaudeDesktopProfileStatus> {
        let _guard = PROFILE_OPERATIONS.lock().await;
        self.validate_claude_profile_credential(id).await?;
        let registration = profile::read_registration(id)?.unwrap_or(ProfileRegistration {
            user_data_dir: managed_path(id),
            account_uuid: None,
        });
        profile::private_directory(&registration.user_data_dir)?;
        profile::write_registration(id, &registration)?;
        open_registered(id, registration).await
    }

    #[tracing::instrument(skip_all, fields(credential_id = id))]
    pub async fn open_claude_desktop_profile(
        &self,
        id: i64,
    ) -> DesktopResult<ClaudeDesktopProfileStatus> {
        let _guard = PROFILE_OPERATIONS.lock().await;
        self.validate_claude_profile_credential(id).await?;
        let registration = profile::read_registration(id)?.ok_or_else(|| {
            ClaudeDesktopProfileError::Conflict("Set up or adopt this Desktop profile first".into())
        })?;
        open_registered(id, registration).await
    }

    /// Adopt in place; never move, copy, or overwrite the user's Desktop login data.
    #[tracing::instrument(skip_all, fields(credential_id = id))]
    pub async fn adopt_claude_desktop_profile(
        &self,
        id: i64,
        path: &Path,
    ) -> DesktopResult<ClaudeDesktopProfileStatus> {
        let _guard = PROFILE_OPERATIONS.lock().await;
        self.validate_claude_profile_credential(id).await?;
        if !path.is_absolute() || !path.join("config.json").is_file() {
            return Err(ClaudeDesktopProfileError::Conflict(
                "Choose an existing absolute Claude Desktop data directory containing config.json"
                    .into(),
            ));
        }
        let path = path
            .canonicalize()
            .context("Resolve Desktop profile path")?;
        let account_uuid = profile::account_uuid(&path)?;
        if account_uuid.is_none() {
            return Err(ClaudeDesktopProfileError::Conflict(
                "Sign into this Desktop profile before adopting it".into(),
            ));
        }
        // A directory must never be registered to two credential rows.
        let root = profile::profile_root();
        if root.exists() {
            for entry in std::fs::read_dir(root).context("Read profile registrations")? {
                let entry = entry.context("Read profile registration entry")?;
                let entry_path = entry.path();
                if entry_path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let Some(other_id) = entry_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .and_then(|value| value.parse::<i64>().ok())
                else {
                    continue;
                };
                if self
                    .get_credential_row(other_id)
                    .await
                    .map_err(anyhow::Error::from)?
                    .is_none()
                {
                    continue;
                }
                if other_id != id
                    && profile::read_registration(other_id)?
                        .is_some_and(|other| other.user_data_dir == path)
                {
                    return Err(ClaudeDesktopProfileError::Conflict(
                        "This Desktop directory is already assigned to another credential".into(),
                    ));
                }
            }
        }
        if let Some(existing) = profile::read_registration(id)? {
            if existing.user_data_dir != path {
                return Err(ClaudeDesktopProfileError::Conflict("This credential already has a Desktop profile; its existing registration is preserved".into()));
            }
        }
        profile::write_registration(
            id,
            &ProfileRegistration {
                user_data_dir: path,
                account_uuid,
            },
        )?;
        status(id).await.map_err(Into::into)
    }

    #[tracing::instrument(skip_all, fields(credential_id = id))]
    pub async fn sync_claude_desktop_sessions(
        &self,
        id: i64,
        range: api_types::ClaudeDesktopSessionRange,
        preview: bool,
    ) -> DesktopResult<ClaudeDesktopSessionSyncResponse> {
        let _guard = PROFILE_OPERATIONS.lock().await;
        self.validate_claude_profile_credential(id).await?;
        let mut registration = profile::read_registration(id)?.ok_or_else(|| {
            ClaudeDesktopProfileError::Conflict("Set up or adopt this Desktop profile first".into())
        })?;
        if !preview
            && profile::running_pid(&registration.user_data_dir)
                .await?
                .is_some()
        {
            return Err(ClaudeDesktopProfileError::Conflict(
                "Quit this Claude Desktop profile before importing local sessions".into(),
            ));
        }
        guard_identity(&registration)?;
        let account = profile::account_uuid(&registration.user_data_dir)?.ok_or_else(|| {
            ClaudeDesktopProfileError::Conflict("Sign into this Desktop profile first".into())
        })?;
        let org = organization(&registration.user_data_dir, &account)?.ok_or_else(|| {
            ClaudeDesktopProfileError::Conflict(
                "Open a Code session in this profile first to establish its organization".into(),
            )
        })?;
        if !preview && registration.account_uuid.is_none() {
            registration.account_uuid = Some(account.clone());
            profile::write_registration(id, &registration)?;
        }
        let recent_days = match range {
            api_types::ClaudeDesktopSessionRange::Week => Some(7),
            api_types::ClaudeDesktopSessionRange::Month => Some(30),
            api_types::ClaudeDesktopSessionRange::All => None,
        };
        let report = sync_sessions_blocking(
            registration.user_data_dir,
            account,
            org,
            recent_days,
            preview,
        )
        .await?;
        Ok(ClaudeDesktopSessionSyncResponse {
            imported: report.imported,
            eligible: report.eligible,
            skipped: report.skipped,
            journal_path: report.journal_path,
        })
    }
}

fn managed_path(id: i64) -> PathBuf {
    profile::profile_root()
        .join("profiles")
        .join(id.to_string())
}

async fn status(id: i64) -> Result<ClaudeDesktopProfileStatus> {
    let registration = profile::read_registration(id)?;
    let path = registration
        .as_ref()
        .map_or_else(|| managed_path(id), |value| value.user_data_dir.clone());
    let account = profile::account_uuid(&path)?;
    let expected = registration
        .as_ref()
        .and_then(|value| value.account_uuid.clone());
    let state = if registration.is_none() {
        ClaudeDesktopProfileState::Unconfigured
    } else if account.is_none() {
        ClaudeDesktopProfileState::LoginRequired
    } else if expected.is_some() && expected != account {
        ClaudeDesktopProfileState::AccountChanged
    } else {
        ClaudeDesktopProfileState::Ready
    };
    Ok(ClaudeDesktopProfileStatus {
        credential_id: id,
        user_data_dir: path.to_string_lossy().into_owned(),
        shared_claude_dir: profile::shared_claude_dir().to_string_lossy().into_owned(),
        account_uuid: account,
        expected_account_uuid: expected,
        state,
        running: profile::running_pid(&path).await?.is_some(),
    })
}

fn guard_identity(registration: &ProfileRegistration) -> DesktopResult<()> {
    let actual = profile::account_uuid(&registration.user_data_dir)?;
    if registration.account_uuid.is_some()
        && actual.is_some()
        && actual != registration.account_uuid
    {
        return Err(ClaudeDesktopProfileError::Conflict(
            "Desktop account changed since this profile was registered".into(),
        ));
    }
    Ok(())
}

async fn open_registered(
    id: i64,
    mut registration: ProfileRegistration,
) -> DesktopResult<ClaudeDesktopProfileStatus> {
    guard_identity(&registration)?;
    if profile::account_uuid(&registration.user_data_dir)?.is_none()
        && profile::other_profile_running(&registration.user_data_dir).await?
    {
        return Err(ClaudeDesktopProfileError::Conflict("Quit the other Claude Desktop profiles before signing into this one so the browser login callback reaches the correct window. Do not log out.".into()));
    }
    if registration.account_uuid.is_none() {
        registration.account_uuid = profile::account_uuid(&registration.user_data_dir)?;
        profile::write_registration(id, &registration)?;
    }
    profile::launch(&registration.user_data_dir).await?;
    status(id).await.map_err(Into::into)
}

fn organization(path: &Path, account: &str) -> Result<Option<String>> {
    anyhow::ensure!(
        !account.is_empty()
            && account
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "Invalid Desktop account identifier"
    );
    let root = path.join("claude-code-sessions").join(account);
    if !root.exists() {
        return Ok(None);
    }
    let mut organizations = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                organizations.push(name);
            }
        }
    }
    // Do not guess which organization owns imported sessions.
    if organizations.len() == 1 {
        Ok(organizations.pop())
    } else {
        Ok(None)
    }
}

async fn sync_sessions_blocking(
    destination: PathBuf,
    account: String,
    org: String,
    days: Option<u32>,
    preview: bool,
) -> Result<crate::io::claude_desktop_sessions::LocalSessionSyncReport> {
    tokio::task::spawn_blocking(move || {
        let mut sources =
            vec![global_infra::paths::home_dir().join("Library/Application Support/Claude")];
        let root = profile::profile_root();
        if root.exists() {
            for entry in std::fs::read_dir(root)? {
                let path = entry?.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                if let Some(id) = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .and_then(|value| value.parse::<i64>().ok())
                {
                    if let Some(registration) = profile::read_registration(id)? {
                        sources.push(registration.user_data_dir);
                    }
                }
            }
        }
        sources.retain(|path| path != &destination);
        sources.sort();
        sources.dedup();
        crate::io::claude_desktop_sessions::sync_local_sessions(
            &profile::shared_claude_dir(),
            &destination,
            &account,
            &org,
            days,
            preview,
            &sources,
        )
    })
    .await
    .context("Local session import worker failed")?
}
