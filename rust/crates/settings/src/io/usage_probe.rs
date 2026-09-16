//! Subscription usage snapshots shared across Claude and Codex credentials.
//!
//! Claude probes use an isolated no-tools Fable print session so setup tokens
//! expose the model-specific weekly window as well as aggregate usage.
use serde::Serialize;

/// Subscription rate-limit status reported by the credential provider.
/// Canonical type lives in `global-types::rate_limit`; this re-export keeps
/// the public surface of `settings` stable.
pub use global_types::RateLimitStatus;

/// State of a single rate-limit window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    /// Fraction of the window consumed; may exceed 1 when overage is allowed.
    pub utilization: f64,
    /// Unix seconds when the window resets.
    pub reset_at: i64,
    pub status: RateLimitStatus,
}

/// Snapshot of one credential's aggregate and model-specific usage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub five_hour: WindowState,
    pub seven_day: WindowState,
    /// Fable weekly allowance, when the provider exposes it.
    pub seven_day_fable: Option<WindowState>,
    pub unified_status: RateLimitStatus,
    /// Which window the server treats as binding right now
    /// (e.g. `five_hour`, `seven_day`, `seven_day_opus`).
    pub representative_claim: Option<String>,
    /// Unix seconds when this snapshot was captured.
    pub probed_at: i64,
}

/// Probe failure modes.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Token rejected as expired or invalid. Caller should mark credential
    /// expired and stop probing until the user re-authenticates.
    #[error("unauthorized (token expired or invalid)")]
    Unauthorized,
    /// A rejected print session may include its reset without utilization.
    #[error("credential is rate limited")]
    RateLimited {
        resets_at: Option<u64>,
        claim: Option<String>,
    },
    /// Unexpected HTTP status other than 200, 401, or 429.
    #[error("unexpected HTTP status {0}")]
    Http(u16),
    /// Network-level failure (connect/timeout/DNS).
    #[error("network error: {0}")]
    Network(String),
    /// The provider response is missing required subscription usage fields.
    #[error("parse error: {0}")]
    Parse(String),
    /// The probe succeeded but persisting the snapshot to the DB failed.
    /// Callers that care about `last_probed_at` advancing (the poll
    /// throttle, the pre-spawn staleness check) must treat this as a
    /// hard failure, not a transient one.
    #[error("persist error: {0}")]
    Persist(String),
}

/// Read all subscription windows through a minimal Claude Code session.
#[tracing::instrument(skip_all)]
pub async fn probe(access_token: &str) -> Result<UsageSnapshot, ProbeError> {
    let snapshot = global_claude::probe_quota(access_token)
        .await
        .map_err(|error| match error {
            global_claude::QuotaProbeError::Unauthorized => ProbeError::Unauthorized,
            global_claude::QuotaProbeError::RateLimited { resets_at, claim } => {
                ProbeError::RateLimited { resets_at, claim }
            }
            global_claude::QuotaProbeError::Parse(message) => ProbeError::Parse(message),
            other => ProbeError::Network(other.to_string()),
        })?;
    let claim = snapshot.representative_claim.as_deref();
    let window = |value: global_claude::QuotaWindow, name: &str| WindowState {
        utilization: value.utilization,
        reset_at: value.resets_at,
        status: if claim == Some(name) {
            snapshot.status.clone()
        } else if value.utilization >= 1.0 {
            RateLimitStatus::Rejected
        } else {
            RateLimitStatus::Allowed
        },
    };
    Ok(UsageSnapshot {
        five_hour: window(snapshot.five_hour, "five_hour"),
        seven_day: window(snapshot.seven_day, "seven_day"),
        seven_day_fable: snapshot
            .seven_day_fable
            .map(|value| window(value, "seven_day_overage_included")),
        unified_status: snapshot.status,
        representative_claim: snapshot.representative_claim,
        probed_at: time::OffsetDateTime::now_utc().unix_timestamp(),
    })
}
