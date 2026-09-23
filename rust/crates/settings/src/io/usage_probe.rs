//! Subscription usage snapshots shared across Claude and Codex credentials.
//!
//! Claude credentials get two isolated no-tools print sessions. The verdict
//! turn runs Mando's default Claude model and alone decides whether the
//! credential is usable. A concurrent Fable turn reads the Fable weekly
//! allowance for display; its outcome never gates selection.
use std::time::Duration;

use serde::Serialize;
use tracing::warn;

/// Model whose allowed/rejected verdict gates credential selection. Matches
/// the default for Mando's own Claude sessions.
const VERDICT_MODEL: &str = "claude-opus-5-5";
/// Claude Code reports the Fable weekly bucket only on a Fable turn.
const FABLE_MODEL: &str = "claude-fable-5-1";
/// How long the Fable turn may run past the verdict before it is dropped,
/// so a slow Fable session never delays credential selection by much.
const FABLE_GRACE: Duration = Duration::from_secs(20);

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

/// Fable weekly allowance from the display-only Fable probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "window")]
pub enum FableUsage {
    /// The Fable turn reported the weekly bucket.
    Measured(WindowState),
    /// Nothing to report (the Fable turn omitted the bucket, or the
    /// credential is not Claude); clears any stored measurement.
    Absent,
    /// The Fable turn failed or ran past its grace period; the stored
    /// measurement stays as last reported.
    Unknown,
}

impl FableUsage {
    pub fn window(&self) -> Option<&WindowState> {
        match self {
            Self::Measured(window) => Some(window),
            Self::Absent | Self::Unknown => None,
        }
    }
}

/// Snapshot of one credential's aggregate and model-specific usage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub five_hour: WindowState,
    pub seven_day: WindowState,
    /// Display only: never feeds `unified_status`.
    pub seven_day_fable: FableUsage,
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

/// Read all subscription windows through minimal Claude Code sessions.
#[tracing::instrument(skip_all)]
pub async fn probe(access_token: &str) -> Result<UsageSnapshot, ProbeError> {
    let verdict = global_claude::probe_quota(access_token, VERDICT_MODEL);
    let fable = global_claude::probe_quota(access_token, FABLE_MODEL);
    tokio::pin!(verdict, fable);
    let mut fable_result = None;
    let verdict_result = loop {
        tokio::select! {
            result = &mut verdict => break result,
            result = &mut fable, if fable_result.is_none() => fable_result = Some(result),
        }
    };
    let snapshot = verdict_result.map_err(|error| match error {
        global_claude::QuotaProbeError::Unauthorized => ProbeError::Unauthorized,
        global_claude::QuotaProbeError::RateLimited { resets_at, claim } => {
            ProbeError::RateLimited { resets_at, claim }
        }
        global_claude::QuotaProbeError::Parse(message) => ProbeError::Parse(message),
        other => ProbeError::Network(other.to_string()),
    })?;
    let fable_result = match fable_result {
        Some(result) => Some(result),
        // On timeout, dropping the unfinished future on return kills its child.
        None => tokio::time::timeout(FABLE_GRACE, &mut fable).await.ok(),
    };
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
        seven_day_fable: fable_usage(fable_result),
        unified_status: snapshot.status,
        representative_claim: snapshot.representative_claim,
        probed_at: time::OffsetDateTime::now_utc().unix_timestamp(),
    })
}

/// Fable claim name Claude Code uses for the Fable weekly bucket.
const FABLE_CLAIM: &str = "seven_day_overage_included";

/// Map the Fable turn's outcome onto the display-only Fable window.
fn fable_usage(
    result: Option<Result<global_claude::QuotaSnapshot, global_claude::QuotaProbeError>>,
) -> FableUsage {
    match result {
        Some(Ok(snapshot)) => match snapshot.seven_day_fable {
            Some(value) => FableUsage::Measured(WindowState {
                utilization: value.utilization,
                reset_at: value.resets_at,
                status: if snapshot.representative_claim.as_deref() == Some(FABLE_CLAIM) {
                    snapshot.status
                } else if value.utilization >= 1.0 {
                    RateLimitStatus::Rejected
                } else {
                    RateLimitStatus::Allowed
                },
            }),
            None => FableUsage::Absent,
        },
        // A rejection without parseable windows still names the exhausted
        // bucket and its reset, which is all the meter needs.
        Some(Err(global_claude::QuotaProbeError::RateLimited {
            resets_at: Some(resets_at),
            claim: Some(claim),
        })) if claim == FABLE_CLAIM => FableUsage::Measured(WindowState {
            utilization: 1.0,
            reset_at: i64::try_from(resets_at).unwrap_or(i64::MAX),
            status: RateLimitStatus::Rejected,
        }),
        Some(Err(error)) => {
            warn!(
                module = "settings",
                error = %error,
                "Fable usage probe failed; keeping the last Fable measurement"
            );
            FableUsage::Unknown
        }
        None => {
            warn!(
                module = "settings",
                grace_secs = FABLE_GRACE.as_secs(),
                "Fable usage probe outlived the verdict grace period; keeping the last Fable measurement"
            );
            FableUsage::Unknown
        }
    }
}

/// Read the verdict for the model being routed without an unrelated display probe.
/// Non-Fable requests preserve the last display measurement and its timestamp.
#[tracing::instrument(skip_all)]
pub async fn probe_model(access_token: &str, model: &str) -> Result<UsageSnapshot, ProbeError> {
    let snapshot = global_claude::probe_quota(access_token, model)
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
    let seven_day_fable = if model.to_ascii_lowercase().contains("fable") {
        snapshot
            .seven_day_fable
            .map_or(FableUsage::Absent, |value| {
                FableUsage::Measured(window(value, FABLE_CLAIM))
            })
    } else {
        FableUsage::Unknown
    };
    Ok(UsageSnapshot {
        five_hour: window(snapshot.five_hour, "five_hour"),
        seven_day: window(snapshot.seven_day, "seven_day"),
        seven_day_fable,
        unified_status: snapshot.status,
        representative_claim: snapshot.representative_claim,
        probed_at: time::OffsetDateTime::now_utc().unix_timestamp(),
    })
}
