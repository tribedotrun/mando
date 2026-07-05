//! Generic credential-runtime methods on `SettingsRuntime` (list, fetch,
//! store, remove, mark-expired, pick-for-worker, cooldown queries).
//!
//! Lives in its own module so `settings_runtime.rs` stays under the file
//! length limit.

use std::collections::HashMap;

use super::settings_runtime::{SettingsError, SettingsResult, SettingsRuntime};

impl SettingsRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn list_credentials(&self) -> Vec<crate::io::credentials::CredentialInfo> {
        match crate::io::credentials::list_all(&self.db_pool).await {
            Ok(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut info = row.to_info();
                    if let Some(last) = row.last_probed_at {
                        let cost =
                            crate::io::credentials::cost_since(&self.db_pool, row.id, last).await;
                        if cost > 0.0 {
                            info.cost_since_probe_usd = Some(cost);
                        }
                    }
                    out.push(info);
                }
                out
            }
            Err(err) => {
                tracing::warn!(module = "credentials", error = %err, "failed to list credentials");
                Vec::new()
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_credential_token(&self, id: i64) -> SettingsResult<Option<String>> {
        crate::io::credentials::get_token_by_id(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_credential_row(
        &self,
        id: i64,
    ) -> SettingsResult<Option<crate::io::credentials::CredentialRow>> {
        crate::io::credentials::get_row_by_id(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn store_credential(
        &self,
        label: &str,
        access_token: &str,
        expires_at: Option<i64>,
    ) -> SettingsResult<i64> {
        let id =
            crate::io::credentials::insert(&self.db_pool, label, access_token, expires_at).await?;
        tracing::info!(module = "credentials", id, "stored credential");
        Ok(id)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_credential_by_label(&self, label: &str) -> SettingsResult<Option<i64>> {
        crate::io::credentials::find_by_label(&self.db_pool, label)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_credential(&self, id: i64) -> SettingsResult<bool> {
        let removed = crate::io::credentials::delete(&self.db_pool, id).await?;
        if removed {
            tracing::info!(module = "credentials", id, "removed credential");
        }
        Ok(removed)
    }

    #[tracing::instrument(skip_all)]
    pub async fn mark_credential_expired(&self, id: i64) -> SettingsResult<bool> {
        crate::io::credentials::mark_expired(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn has_any_credentials(&self) -> SettingsResult<bool> {
        crate::io::credentials::has_any(&self.db_pool)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn pick_worker_credential(
        &self,
        caller_filter: Option<&str>,
    ) -> SettingsResult<Option<(i64, String)>> {
        crate::io::credentials::pick_for_worker(&self.db_pool, caller_filter)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn earliest_credential_cooldown_remaining_secs(&self) -> SettingsResult<i64> {
        // Wrap the anyhow error from the io layer into the typed
        // SettingsError envelope so the public SettingsRuntime API
        // stays C2-compliant (no raw anyhow on the boundary).
        crate::io::credentials::earliest_cooldown_remaining_secs(&self.db_pool)
            .await
            .map_err(SettingsError::Other)
    }

    #[tracing::instrument(skip_all)]
    pub async fn credential_labels_by_ids(
        &self,
        ids: &[i64],
    ) -> SettingsResult<HashMap<i64, String>> {
        crate::io::credentials::labels_by_ids(&self.db_pool, ids)
            .await
            .map_err(Into::into)
    }

    /// Resolve an explicit pick target. Returns `None` when neither `id` nor
    /// `label` is set (caller should auto-pick). Errors when both are set or
    /// when `label` does not exist.
    #[tracing::instrument(skip_all)]
    pub async fn resolve_credential_pick_id(
        &self,
        id: Option<i64>,
        label: Option<&str>,
    ) -> SettingsResult<Option<i64>> {
        match (id, label.map(str::trim).filter(|s| !s.is_empty())) {
            (Some(_), Some(_)) => Err(SettingsError::Other(anyhow::anyhow!(
                "specify only one of id or label"
            ))),
            (Some(id), None) => Ok(Some(id)),
            (None, Some(label)) => self.find_credential_by_label(label).await,
            (None, None) => Ok(None),
        }
    }

    /// Pick a specific Claude credential by id or label. Honors the caller's
    /// explicit choice even when the row is expired or rate-limited.
    #[tracing::instrument(skip_all)]
    pub async fn pick_claude_credential_explicit(
        &self,
        id: Option<i64>,
        label: Option<&str>,
    ) -> SettingsResult<Option<(i64, String, String)>> {
        let Some(resolved_id) = self.resolve_credential_pick_id(id, label).await? else {
            return Ok(None);
        };
        let row = self.get_credential_row(resolved_id).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.provider != "claude" {
            return Ok(None);
        }
        Ok(Some((resolved_id, row.access_token, row.label)))
    }
}
