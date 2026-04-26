use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CredentialInfo, CredentialUsageSnapshot};

// ── Daemon-only routes promoted from Value in PR #855 ──────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCreatedResponse {
    pub artifact_id: i64,
    pub task_id: i64,
    pub media: Vec<crate::ArtifactMedia>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SummaryCreatedResponse {
    pub artifact_id: i64,
    pub task_id: i64,
}

// ── Credential response envelopes ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialListResponse {
    pub credentials: Vec<CredentialInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialsListResponse {
    pub credentials: Vec<CredentialInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialProbeResponse {
    pub ok: bool,
    pub snapshot: CredentialUsageSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProbeCredentialResponse {
    pub ok: bool,
    pub snapshot: Option<CredentialUsageSnapshot>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TokenResponse {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialTokenResponse {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialMutationResponse {
    pub ok: bool,
    pub error: Option<String>,
}

/// A credential the daemon picked for a caller. Carries the secret token —
/// only returned to authenticated callers (Bearer token required by the
/// route). Mirrors the worker-spawn injection: the consumer exports
/// `token` into the environment as `CLAUDE_CODE_OAUTH_TOKEN`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialPick {
    pub id: i64,
    pub label: String,
    pub token: String,
}

/// POST /api/credentials/pick -- response.
///
/// `pick` is `None` when no credential is usable right now (table empty,
/// all expired, or all in rate-limit cooldown). The shell wrapper treats
/// `None` as "fall through to ambient login" rather than an error.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialPickResponse {
    pub pick: Option<CredentialPick>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetupTokenResponse {
    pub ok: bool,
    pub id: Option<i64>,
    pub label: Option<String>,
}
