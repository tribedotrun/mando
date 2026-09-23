//! Settings boundary for shared routing and external process lease management.
use super::settings_runtime::{SettingsResult, SettingsRuntime};
use crate::io::{credential_leases, credential_routing};
use api_types::{
    CredentialLeaseCountsResponse, CredentialLeaseHeartbeatRequest, CredentialLeaseResponse,
    CredentialRouteRequest, CredentialRouteResponse, CredentialRoutingStatusResponse,
};

impl SettingsRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn route_claude_credential(
        &self,
        request: &CredentialRouteRequest,
    ) -> SettingsResult<CredentialRouteResponse> {
        credential_routing::route(&self.db_pool, request)
            .await
            .map_err(Into::into)
    }
    #[tracing::instrument(skip_all)]
    pub async fn heartbeat_claude_lease(
        &self,
        request: &CredentialLeaseHeartbeatRequest,
    ) -> SettingsResult<CredentialLeaseResponse> {
        let expires_at = credential_leases::heartbeat(&self.db_pool, request).await?;
        Ok(CredentialLeaseResponse {
            ok: true,
            expires_at: Some(expires_at),
        })
    }
    #[tracing::instrument(skip_all)]
    pub async fn release_claude_lease(
        &self,
        launch: &str,
    ) -> SettingsResult<CredentialLeaseResponse> {
        credential_leases::release(&self.db_pool, launch).await?;
        Ok(CredentialLeaseResponse {
            ok: true,
            expires_at: None,
        })
    }
    #[tracing::instrument(skip_all)]
    pub async fn claude_lease_counts(&self) -> SettingsResult<CredentialLeaseCountsResponse> {
        let profiles = credential_leases::counts(
            &mut *self.db_pool.acquire().await.map_err(anyhow::Error::from)?,
            time::OffsetDateTime::now_utc().unix_timestamp(),
            "",
        )
        .await?;
        Ok(CredentialLeaseCountsResponse { profiles })
    }
    #[tracing::instrument(skip_all)]
    pub async fn claude_routing_status(&self) -> SettingsResult<CredentialRoutingStatusResponse> {
        credential_routing::status(&self.db_pool)
            .await
            .map_err(Into::into)
    }
}
