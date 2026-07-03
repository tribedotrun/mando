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
