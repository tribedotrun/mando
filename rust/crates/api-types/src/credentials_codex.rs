//! Codex credential wire types.
//!
//! See PR #1006. Codex credentials live in the same `credentials` table as
//! Claude OAuth tokens but carry additional per-account metadata
//! (`account_id`, `plan_type`, credits, refresh tokens). The Claude-side
//! types in `models.rs` stay backwards compatible; everything Codex-specific
//! lives here.

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

/// GET /api/credentials/codex/active — read `~/.codex/auth.json`'s account_id
/// and report whether any stored Codex credential matches.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexActiveResponse {
    pub active_account_id: Option<String>,
    pub matched_credential_id: Option<i64>,
}

/// POST /api/credentials/{id}/codex-activate
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexActivateResponse {
    pub ok: bool,
    pub account_id: String,
}
