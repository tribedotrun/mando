//! Daily Desktop billing refresh independent of inference usage probes.
use super::{ClaudeDesktopProfileError, SettingsRuntime};
use crate::io::{
    claude_desktop_profile as profile, claude_subscription_probe as probe,
    claude_subscription_store as store,
};
use anyhow::{Context, Result};
use api_types::ClaudeSubscriptionInfo;
use std::path::PathBuf;

static SUBSCRIPTION_REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ProfileContext {
    path: PathBuf,
    account: String,
}

fn profile_context(id: i64) -> Result<Option<ProfileContext>> {
    let Some(registration) = profile::read_registration(id)? else {
        return Ok(None);
    };
    let account = profile::account_uuid(&registration.user_data_dir)?
        .context("Sign into this Claude Desktop profile to refresh its plan")?;
    let expected = registration
        .account_uuid
        .context("Open this Desktop profile through Mando to record its account first")?;
    anyhow::ensure!(
        expected == account,
        "Desktop account changed. Restore the linked account before refreshing its plan."
    );
    Ok(Some(ProfileContext {
        path: registration.user_data_dir,
        account,
    }))
}

pub(crate) fn unavailable(error: String) -> ClaudeSubscriptionInfo {
    ClaudeSubscriptionInfo {
        plan_name: None,
        billing_interval: None,
        status: None,
        renews_at: None,
        ends_at: None,
        ends_before: None,
        next_charge_date: None,
        checked_at: None,
        attempted_at: None,
        error: Some(error),
        keychain_access_required: false,
    }
}

impl SettingsRuntime {
    /// Only the explicit UI permission action may open a native Keychain dialog.
    #[tracing::instrument(skip_all)]
    pub async fn authorize_claude_desktop_keychain(&self) -> Result<(), ClaudeDesktopProfileError> {
        crate::io::claude_desktop_auth::authorize_keychain()
            .await
            .map_err(Into::into)
    }
    /// Fetch only metadata when due; failed attempts are also limited to daily.
    #[tracing::instrument(skip_all)]
    pub async fn refresh_due_claude_subscriptions(&self) -> Result<bool, super::SettingsError> {
        let _guard = SUBSCRIPTION_REFRESH.lock().await;
        let mut changed = false;
        for row in crate::io::credentials::list_all(&self.db_pool).await? {
            if row.provider != "claude" {
                continue;
            }
            // Unlinked credentials have no Desktop billing source.
            if matches!(profile::read_registration(row.id), Ok(None)) {
                continue;
            }
            let previous = store::get(&self.db_pool, row.id)
                .await?
                .map(|row| row.attempted_at);
            match self.refresh_subscription_locked(row.id, false).await {
                Ok(info) => changed |= info.attempted_at != previous,
                Err(error) => {
                    changed = true;
                    tracing::error!(module="claude-subscription", credential_id=row.id, %error, "Failed to persist Claude subscription refresh");
                }
            }
        }
        Ok(changed)
    }

    #[tracing::instrument(skip_all, fields(credential_id=id))]
    pub async fn refresh_claude_subscription(
        &self,
        id: i64,
    ) -> Result<ClaudeSubscriptionInfo, ClaudeDesktopProfileError> {
        let row = self
            .get_credential_row(id)
            .await
            .map_err(anyhow::Error::from)?;
        if row.is_none_or(|row| row.provider != "claude") {
            return Err(ClaudeDesktopProfileError::NotFound(id));
        }
        let _guard = SUBSCRIPTION_REFRESH.lock().await;
        self.refresh_subscription_locked(id, true)
            .await
            .map_err(Into::into)
    }

    async fn refresh_subscription_locked(
        &self,
        id: i64,
        force: bool,
    ) -> Result<ClaudeSubscriptionInfo> {
        let context = profile_context(id);
        let account = context
            .as_ref()
            .ok()
            .and_then(|context| context.as_ref())
            .map_or("", |context| context.account.as_str());
        let attempted = now_ms();
        if !store::claim(&self.db_pool, id, account, attempted, force).await? {
            return store::get(&self.db_pool, id)
                .await?
                .context("Subscription cache disappeared after daily guard")
                .map(store::SubscriptionRow::into_info);
        }
        let result = async {
            let context =
                context?.context("Link a signed-in Claude Desktop profile to refresh its plan")?;
            let snapshot = probe::probe(&context.path, &context.account).await?;
            let current =
                profile_context(id)?.context("Desktop profile was unlinked during refresh")?;
            anyhow::ensure!(
                current.account == context.account && current.path == context.path,
                "Desktop account changed during subscription refresh"
            );
            anyhow::ensure!(
                snapshot.account_uuid == context.account,
                "Subscription response belongs to a different Desktop account"
            );
            Ok::<_, anyhow::Error>(snapshot)
        }
        .await;
        match result {
            Ok(snapshot) => {
                store::succeed(&self.db_pool, id, attempted, &snapshot, now_ms()).await?;
                tracing::info!(
                    module = "claude-subscription",
                    credential_id = id,
                    "Claude subscription refreshed"
                );
            }
            Err(error) => {
                let message = format!("{error:#}");
                let keychain_access_required = error
                    .downcast_ref::<crate::io::claude_desktop_auth::KeychainAccessRequired>()
                    .is_some();
                store::fail(
                    &self.db_pool,
                    id,
                    attempted,
                    &message,
                    keychain_access_required,
                )
                .await?;
                tracing::warn!(module="claude-subscription", credential_id=id, error=%message, "Claude subscription refresh failed");
            }
        }
        store::get(&self.db_pool, id)
            .await?
            .context("Subscription refresh result missing")
            .map(store::SubscriptionRow::into_info)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn subscription_infos(
        &self,
    ) -> Result<std::collections::HashMap<i64, ClaudeSubscriptionInfo>, super::SettingsError> {
        let mut out = std::collections::HashMap::new();
        for row in store::list(&self.db_pool).await? {
            let id = row.credential_id;
            let info = match profile_context(row.credential_id) {
                Ok(Some(context)) if context.account == row.account_uuid => row.into_info(),
                Ok(None) => continue,
                Ok(Some(_)) => unavailable(
                    "Desktop account changed; subscription details need refreshing.".into(),
                ),
                Err(error) => unavailable(format!("{error:#}")),
            };
            out.insert(id, info);
        }
        Ok(out)
    }
}

fn now_ms() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}
