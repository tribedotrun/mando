//! Authenticated profile routing and independent terminal process leases.
use crate::response::{error_response, internal_error, ApiError};
use crate::{ApiRouter, AppState};
use axum::{extract::State, http::StatusCode, Json};

pub(crate) fn credential_routing_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/credentials/route",
        transport = Json,
        auth = Protected,
        handler = route_credential,
        body = api_types::CredentialRouteRequest,
        res = api_types::CredentialRouteResponse,
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/leases/heartbeat",
        transport = Json,
        auth = Protected,
        handler = heartbeat_lease,
        body = api_types::CredentialLeaseHeartbeatRequest,
        res = api_types::CredentialLeaseResponse,
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/leases/release",
        transport = Json,
        auth = Protected,
        handler = release_lease,
        body = api_types::CredentialLeaseReleaseRequest,
        res = api_types::CredentialLeaseResponse,
    );
    let router = crate::api_route!(
        router,
        GET "/api/credentials/leases",
        transport = Json,
        auth = Protected,
        handler = lease_counts,
        res = api_types::CredentialLeaseCountsResponse,
    );
    crate::api_route!(
        router,
        GET "/api/credentials/routing",
        transport = Json,
        auth = Protected,
        handler = routing_status,
        res = api_types::CredentialRoutingStatusResponse,
    )
}

fn validate_launch(launch: &str) -> Result<(), ApiError> {
    if launch.is_empty()
        || launch.len() > 128
        || !launch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "launch_id must be a nonempty alphanumeric or UUID identifier",
        ));
    }
    Ok(())
}

async fn route_credential(
    State(state): State<AppState>,
    Json(body): Json<api_types::CredentialRouteRequest>,
) -> Result<Json<api_types::CredentialRouteResponse>, ApiError> {
    validate_launch(&body.launch_id)?;
    if body
        .min_headroom_percent
        .is_some_and(|n| !n.is_finite() || !(0.0..=100.0).contains(&n))
        || body.profile.as_ref().is_some_and(|p| p.trim().is_empty())
    {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "invalid headroom percentage or profile",
        ));
    }
    let result = state
        .settings
        .route_claude_credential(&body)
        .await
        .map_err(|e| internal_error(e, "failed to route Claude profile"))?;
    tracing::info!(module = "credentials", launch_id=%body.launch_id, selected_id=result.pick.as_ref().map(|p| p.id), dry_run=body.dry_run, reason=%result.reason, "Claude profile routing completed");
    if !body.dry_run {
        state.bus.send(global_bus::BusPayload::Credentials(None));
    }
    Ok(Json(result))
}
async fn heartbeat_lease(
    State(state): State<AppState>,
    Json(body): Json<api_types::CredentialLeaseHeartbeatRequest>,
) -> Result<Json<api_types::CredentialLeaseResponse>, ApiError> {
    validate_launch(&body.launch_id)?;
    if body.pid == 0 || body.cwd.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "pid and cwd are required",
        ));
    }
    state
        .settings
        .heartbeat_claude_lease(&body)
        .await
        .map(Json)
        .map_err(|e| internal_error(e, "failed to renew Claude lease"))
}
async fn release_lease(
    State(state): State<AppState>,
    Json(body): Json<api_types::CredentialLeaseReleaseRequest>,
) -> Result<Json<api_types::CredentialLeaseResponse>, ApiError> {
    validate_launch(&body.launch_id)?;
    let result = state
        .settings
        .release_claude_lease(&body.launch_id)
        .await
        .map_err(|e| internal_error(e, "failed to release Claude lease"))?;
    state.bus.send(global_bus::BusPayload::Credentials(None));
    Ok(Json(result))
}
async fn lease_counts(
    State(state): State<AppState>,
) -> Result<Json<api_types::CredentialLeaseCountsResponse>, ApiError> {
    state
        .settings
        .claude_lease_counts()
        .await
        .map(Json)
        .map_err(|e| internal_error(e, "failed to read Claude lease counts"))
}
async fn routing_status(
    State(state): State<AppState>,
) -> Result<Json<api_types::CredentialRoutingStatusResponse>, ApiError> {
    state
        .settings
        .claude_routing_status()
        .await
        .map(Json)
        .map_err(|e| internal_error(e, "failed to read Claude routing status"))
}
