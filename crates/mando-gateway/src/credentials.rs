//! Credential manager -- thin wrapper around DB queries for setup-token
//! credential CRUD. No Keychain polling, no token refresh.
//!
//! When no credentials exist, workers use the host's ambient Claude Code login.

use sqlx::SqlitePool;
use tracing::{info, warn};

/// Manages setup-token credentials backed by SQLite.
#[derive(Clone)]
pub struct CredentialManager {
    pool: SqlitePool,
}

impl CredentialManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// List all stored credentials (sanitized: no tokens exposed).
    pub async fn list(&self) -> Vec<mando_db::queries::credentials::CredentialInfo> {
        match mando_db::queries::credentials::list_all(&self.pool).await {
            Ok(rows) => rows.iter().map(|r| r.to_info()).collect(),
            Err(e) => {
                warn!(module = "credentials", error = %e, "failed to list credentials");
                Vec::new()
            }
        }
    }

    /// Store a setup-token credential.
    pub async fn store(
        &self,
        label: &str,
        access_token: &str,
        expires_at: Option<i64>,
    ) -> anyhow::Result<i64> {
        let id =
            mando_db::queries::credentials::insert(&self.pool, label, access_token, expires_at)
                .await?;
        info!(module = "credentials", label, id, "stored credential");
        Ok(id)
    }

    /// Remove a credential by ID.
    pub async fn remove(&self, id: i64) -> anyhow::Result<bool> {
        let removed = mando_db::queries::credentials::delete(&self.pool, id).await?;
        if removed {
            info!(module = "credentials", id, "removed credential");
        }
        Ok(removed)
    }
}
