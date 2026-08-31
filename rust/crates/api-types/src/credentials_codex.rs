//! Codex credential wire types.
//!
//! Codex credentials live in the same `credentials` table as Claude OAuth
//! tokens but carry additional per-account metadata (`account_id`,
//! `plan_type`, credits, refresh tokens). Codex-specific request/response
//! types live here; the shared list payload is `CredentialInfo` in
//! `credentials.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which credential provider a row belongs to. New rows default to `claude`
/// at the DB layer for backwards compatibility with pre-PR-1006 inserts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialProvider {
    Claude,
    Codex,
}

/// Codex-only fields rendered alongside the existing `CredentialInfo`. None
/// of these are populated for Claude rows.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCredentialDetails {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub credits_balance: Option<String>,
    pub credits_unlimited: bool,
}

/// One available Codex rate-limit reset credit.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexResetCredit {
    pub title: String,
    pub description: Option<String>,
    /// Unix seconds when the credit expires.
    pub expires_at: i64,
    /// Unix seconds when the credit was granted, when OpenAI returns it.
    pub granted_at: Option<i64>,
}

/// GET /api/credentials/codex/{id}/reset-credits
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexResetCreditsResponse {
    pub available_count: i64,
    pub total_earned_count: i64,
    pub credits: Vec<CodexResetCredit>,
}

/// POST /api/credentials/codex
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddCodexCredentialRequest {
    pub label: String,
    /// Raw contents of an OpenAI Codex `auth.json` file. Validated server-side.
    pub auth_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddCodexCredentialResponse {
    pub ok: bool,
    pub id: i64,
    pub label: String,
    pub account_id: String,
    pub plan_type: Option<String>,
    /// Set when the add succeeded but something about the pasted session
    /// is worth a human's attention (see [`CodexCredentialAddWarning`]).
    pub warning: Option<CodexCredentialAddWarning>,
}

/// Non-fatal warnings surfaced on a successful [`AddCodexCredentialResponse`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CodexCredentialAddWarning {
    /// The pasted `account_id` matches the ambient `~/.codex` login
    /// (`account_id` equal, tokens different — a separate session for the
    /// same account). Pool usage will share that account's rate limits
    /// with the user's personal Codex session.
    SharedAccountWithAmbient { message: String },
    /// The add-time forced refresh (session-liveness validation) failed
    /// transiently (network/5xx); the add proceeded with the pasted
    /// tokens as-is, unvalidated.
    ValidationSkippedTransient { message: String },
    /// The pasted (or rotated) tokens were persisted successfully, but the
    /// synchronous usage/plan-info probe that normally runs right after
    /// failed. The credential is safe to use — the next scheduled poll
    /// tick seeds the usage snapshot — this is purely informational.
    UsageProbeFailed { message: String },
}

/// A Codex credential picked for per-process env injection.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCredentialPick {
    pub id: i64,
    pub label: String,
    pub access_token: String,
    pub account_id: String,
    /// Serialized `auth.json` for a per-process `CODEX_HOME` (ChatGPT OAuth
    /// tokens are JWT-shaped and are misclassified when passed via
    /// `CODEX_ACCESS_TOKEN`; the shell wrapper points `CODEX_HOME` at a temp
    /// dir with this file plus symlinks back to `~/.codex` session state).
    pub auth_json: String,
}

/// POST /api/credentials/codex/{id}/auth — replace the stored session on
/// an existing Codex credential with a freshly captured `auth.json` for the
/// same ChatGPT account (same label, same row). Responds with the shared
/// [`AddCodexCredentialResponse`] shape since it runs the same
/// validate-then-store pipeline as the add endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCodexCredentialAuthRequest {
    /// Raw contents of an OpenAI Codex `auth.json` file. Validated server-side.
    pub auth_json: String,
}

/// POST /api/credentials/codex/sync — persist refreshed tokens from a
/// per-process `CODEX_HOME/auth.json` back into the credential row.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncCodexCredentialRequest {
    pub credential_id: i64,
    pub auth_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncCodexCredentialResponse {
    pub ok: bool,
}

/// POST /api/credentials/codex/app/use — swap a stored pool account into
/// the ChatGPT desktop app's shared Codex auth slot.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct CodexDesktopAppUseRequest {
    pub label: String,
    /// Optional caller-side `CODEX_HOME`. The CLI forwards its environment
    /// so moving orchestration into the daemon does not change that override.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub codex_home: Option<String>,
    /// Process id of a thin CLI caller. The daemon excludes that process
    /// from the external-Codex warning scan, matching the former in-process
    /// CLI behavior without trusting it for any process termination.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub caller_pid: Option<u32>,
}

/// POST /api/credentials/codex/app/restore.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct CodexDesktopAppRestoreRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub codex_home: Option<String>,
}

/// GET /api/credentials/codex/app/status query parameters.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct CodexDesktopAppStatusQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub codex_home: Option<String>,
}

/// Result of a successful use/restore operation. Warnings preserve the CLI's
/// best-effort process and recovery messages without making them fatal.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexDesktopAppOperationResponse {
    pub message: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CodexDesktopAppMode {
    Pool,
    Ambient,
    None,
}

/// GET /api/credentials/codex/app/status.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexDesktopAppStatusResponse {
    pub mode: CodexDesktopAppMode,
    pub active_label: Option<String>,
    pub credential_id: Option<i64>,
    pub slot_account_id: Option<String>,
    pub can_restore: bool,
}

/// POST /api/credentials/pick and POST /api/credentials/codex/pick.
///
/// All fields optional. When both `id` and `label` are absent, the daemon
/// auto-picks the best-available credential. When one is set, that exact row
/// is used even if expired or rate-limited; manually disabled rows are still
/// excluded from explicit picks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct CredentialPickRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub label: Option<String>,
}

/// POST /api/credentials/codex/pick — response.
///
/// `pick` is `None` when no Codex credential is usable right now (table
/// empty, all expired, or all in rate-limit cooldown). The shell wrapper
/// treats `None` as "fall through to ambient login" rather than an error.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexCredentialPickResponse {
    pub pick: Option<CodexCredentialPick>,
}

/// Status of a single-flight background `codex login` browser-OAuth flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CodexLoginStatus {
    Pending,
    Success,
    Failed,
    Cancelled,
}

impl CodexLoginStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for CodexLoginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

/// POST /api/credentials/codex/login/start — begin a 1-click browser OAuth
/// login. Cancels and replaces any already-pending flow.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct StartCodexLoginRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub label: Option<String>,
    /// Row-scoped re-login: the captured session must belong to this
    /// existing credential's ChatGPT account, and the row's label is kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub credential_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartCodexLoginResponse {
    pub ok: bool,
    pub login_id: String,
}

/// Snapshot of the single in-flight (or most-recently-finished) Codex
/// browser login flow. `auth_url` populates once `codex login` prints the
/// callback URL; `label` and `warning` populate on `Success`; `error`
/// populates on `Failed`. `credential_id` is set for a row-scoped re-login
/// so the UI can render the pending state on that specific row.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexLoginFlowInfo {
    pub login_id: String,
    pub status: CodexLoginStatus,
    pub credential_id: Option<i64>,
    pub auth_url: Option<String>,
    pub label: Option<String>,
    pub warning: Option<CodexCredentialAddWarning>,
    pub error: Option<String>,
}

/// GET /api/credentials/codex/login/current — `flow` is `None` when no
/// login flow has run since the daemon started.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexLoginStatusResponse {
    pub flow: Option<CodexLoginFlowInfo>,
}

/// POST /api/credentials/codex/login/cancel — `cancelled` is `false` when
/// there was no pending flow to cancel.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelCodexLoginResponse {
    pub ok: bool,
    pub cancelled: bool,
}
