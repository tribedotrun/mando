//! Local Claude launch routing and renewable process leases.
use crate::CredentialPick;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialRouteRequest {
    pub launch_id: String,
    pub current_credential_id: Option<i64>,
    pub profile: Option<String>,
    pub model: Option<String>,
    pub dry_run: bool,
    pub min_headroom_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialRouteCandidate {
    pub credential_id: i64,
    pub label: String,
    pub eligible: bool,
    pub reason: String,
    pub remaining_percent: Option<f64>,
    pub weekly_reset_at: Option<i64>,
    pub live_sessions: i64,
    pub pending_launches: i64,
    pub headroom_percent: Option<f64>,
    pub last_probed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialRouteResponse {
    pub pick: Option<CredentialPick>,
    pub reason: String,
    pub candidates: Vec<CredentialRouteCandidate>,
    pub reservation_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialLeaseHeartbeatRequest {
    pub launch_id: String,
    pub credential_id: i64,
    pub session_id: Option<String>,
    pub pid: u32,
    pub cwd: String,
    pub model: Option<String>,
    /// Only the newly started child confirms its target reservation.
    pub confirm_reservation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialLeaseReleaseRequest {
    pub launch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialLeaseResponse {
    pub ok: bool,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialLeaseCount {
    pub credential_id: i64,
    pub live_sessions: i64,
    pub pending_launches: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialLeaseCountsResponse {
    pub profiles: Vec<CredentialLeaseCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialRoutingStatusResponse {
    pub candidates: Vec<CredentialRouteCandidate>,
}
