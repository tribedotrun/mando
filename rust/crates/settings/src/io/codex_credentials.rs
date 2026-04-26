//! Codex `auth.json` parsing, JWT claim decoding, and re-serialization.
//!
//! See PR #1006. The shape we ingest is the file produced by `codex login`
//! at `$CODEX_HOME/auth.json` (default `~/.codex/auth.json`). Only chatgpt
//! mode is supported; api-key mode has no plan-limit signal and is rejected
//! at the parse layer.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Parsed Codex `auth.json` content. Only the fields we need are extracted.
#[derive(Debug, Clone)]
pub struct ParsedCodexAuth {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub account_id: Option<String>,
    pub last_refresh: Option<String>,
}

/// Claims extracted from the `id_token` JWT. We only read the middle
/// (payload) segment; signature verification is unnecessary because the
/// token was just issued by OpenAI's IdP for this user, and any tampering
/// would invalidate the access_token + refresh_token siblings.
#[derive(Debug, Clone)]
pub struct CodexJwtClaims {
    pub plan_type: Option<String>,
    pub account_id: Option<String>,
    /// Unix seconds when the JWT expires.
    pub exp: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthJsonError {
    #[error("auth.json is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("auth_mode {0:?} is not supported (need chatgpt)")]
    UnsupportedAuthMode(String),
    #[error("auth.json is missing the `tokens.{0}` field")]
    MissingField(&'static str),
    #[error("id_token is malformed: {0}")]
    MalformedIdToken(String),
}

/// Owned shape used to serialize back to disk. Mirrors what `codex login`
/// would write so a Codex client picks the file up cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthJsonOwned {
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    openai_api_key: Option<String>,
    auth_mode: String,
    tokens: AuthJsonTokensOwned,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_refresh: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthJsonTokensOwned {
    id_token: String,
    access_token: String,
    refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
}

/// Parse a raw `auth.json` blob. Rejects api-key mode and any shape
/// missing the three OAuth tokens.
pub fn parse_auth_json(content: &str) -> Result<ParsedCodexAuth, AuthJsonError> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|e| AuthJsonError::InvalidJson(e.to_string()))?;
    let auth_mode = value
        .get("auth_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if auth_mode != "chatgpt" {
        return Err(AuthJsonError::UnsupportedAuthMode(auth_mode.to_string()));
    }
    let tokens = value
        .get("tokens")
        .ok_or(AuthJsonError::MissingField("tokens"))?;

    let access_token = required_string(tokens, "access_token", "access_token")?;
    let refresh_token = required_string(tokens, "refresh_token", "refresh_token")?;
    let id_token = required_string(tokens, "id_token", "id_token")?;
    let account_id = tokens
        .get("account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let last_refresh = value
        .get("last_refresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ParsedCodexAuth {
        access_token,
        refresh_token,
        id_token,
        account_id,
        last_refresh,
    })
}

fn required_string(
    obj: &serde_json::Value,
    key: &str,
    field: &'static str,
) -> Result<String, AuthJsonError> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or(AuthJsonError::MissingField(field))
}

/// Decode the JWT middle segment (no signature verification) and pull the
/// claims we care about. Looks first in the nested
/// `https://api.openai.com/auth` claim, then falls back to top-level keys.
pub fn decode_id_token_claims(id_token: &str) -> Result<CodexJwtClaims, AuthJsonError> {
    let segments: Vec<&str> = id_token.split('.').collect();
    if segments.len() != 3 {
        return Err(AuthJsonError::MalformedIdToken(format!(
            "expected 3 segments, got {}",
            segments.len()
        )));
    }
    let payload_b64 = segments[1];
    let bytes = base64_url_decode(payload_b64)
        .map_err(|e| AuthJsonError::MalformedIdToken(format!("base64 decode: {e}")))?;
    let payload: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AuthJsonError::MalformedIdToken(format!("json parse: {e}")))?;

    let nested = payload
        .get("https://api.openai.com/auth")
        .and_then(|v| v.as_object());

    let plan_type = nested
        .and_then(|n| n.get("chatgpt_plan_type"))
        .or_else(|| payload.get("chatgpt_plan_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let account_id = nested
        .and_then(|n| n.get("chatgpt_account_id"))
        .or_else(|| payload.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let exp = payload.get("exp").and_then(|v| v.as_i64());

    Ok(CodexJwtClaims {
        plan_type,
        account_id,
        exp,
    })
}

/// Decode base64url (RFC 4648 §5). Padding is optional.
fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input.trim_end_matches('='))
        .map_err(|e| e.to_string())
}

// ── DB helpers (mutators specific to provider='codex' rows) ──────────────

/// Insert a Codex credential carrying the full token triple, account id,
/// and plan-type metadata.
#[allow(clippy::too_many_arguments)]
pub async fn insert_codex(
    pool: &SqlitePool,
    label: &str,
    access_token: &str,
    refresh_token: &str,
    id_token: &str,
    account_id: &str,
    plan_type: Option<&str>,
    expires_at: Option<i64>,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO credentials (
            label, access_token, refresh_token, id_token, account_id,
            plan_type, provider, expires_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'codex', ?7, datetime('now'))
         RETURNING id",
    )
    .bind(label)
    .bind(access_token)
    .bind(refresh_token)
    .bind(id_token)
    .bind(account_id)
    .bind(plan_type)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Persist refreshed Codex tokens. `id_token` is optional because OpenAI's
/// refresh response sometimes omits it; we keep the previously-stored value
/// in that case.
pub async fn update_codex_tokens(
    pool: &SqlitePool,
    id: i64,
    access_token: &str,
    refresh_token: &str,
    id_token: Option<&str>,
    expires_at: Option<i64>,
) -> Result<bool> {
    let result = match id_token {
        Some(it) => {
            sqlx::query(
                "UPDATE credentials
                 SET access_token = ?1, refresh_token = ?2, id_token = ?3,
                     expires_at = ?4, updated_at = datetime('now')
                 WHERE id = ?5 AND provider = 'codex'",
            )
            .bind(access_token)
            .bind(refresh_token)
            .bind(it)
            .bind(expires_at)
            .bind(id)
            .execute(pool)
            .await?
        }
        None => {
            sqlx::query(
                "UPDATE credentials
                 SET access_token = ?1, refresh_token = ?2,
                     expires_at = ?3, updated_at = datetime('now')
                 WHERE id = ?4 AND provider = 'codex'",
            )
            .bind(access_token)
            .bind(refresh_token)
            .bind(expires_at)
            .bind(id)
            .execute(pool)
            .await?
        }
    };
    Ok(result.rows_affected() > 0)
}

/// Persist plan/credits info on a Codex credential after a successful probe.
pub async fn update_codex_plan_and_credits(
    pool: &SqlitePool,
    id: i64,
    plan_type: Option<&str>,
    credits_balance: Option<&str>,
    credits_unlimited: bool,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE credentials
         SET plan_type = ?1, credits_balance = ?2, credits_unlimited = ?3,
             updated_at = datetime('now')
         WHERE id = ?4 AND provider = 'codex'",
    )
    .bind(plan_type)
    .bind(credits_balance)
    .bind(if credits_unlimited { 1 } else { 0 })
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Look up the credential id for a Codex `account_id`. Used by activation
/// to compute "currently active" without a stored flag.
pub async fn find_codex_id_by_account(pool: &SqlitePool, account_id: &str) -> Result<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM credentials WHERE provider = 'codex' AND account_id = ?")
            .bind(account_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

// ── Filesystem helpers (read/write ~/.codex/auth.json) ───────────────────

/// Default path for the user's Codex auth file, honoring `CODEX_HOME`.
pub fn default_auth_json_path() -> std::path::PathBuf {
    let home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = std::path::PathBuf::from(h);
                p.push(".codex");
                p
            })
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".codex"));
    home.join("auth.json")
}

/// Why we couldn't read an active account_id from the local auth file.
/// Distinguishes "not logged in / no Codex" (returned to the UI as "no active
/// account") from "file is corrupt or unreadable" (surfaced as a 500 so the
/// user knows the badge can't be trusted, rather than silently disappearing).
#[derive(Debug, thiserror::Error)]
pub enum ReadActiveError {
    #[error("auth.json read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] AuthJsonError),
}

/// Read the `account_id` currently in effect for the local Codex CLI/desktop.
/// Returns `Ok(None)` only when the file is genuinely absent. Read errors,
/// JSON parse errors, and `apikey`-mode files return `Err`.
pub fn read_active_account_id(path: &std::path::Path) -> Result<Option<String>, ReadActiveError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(ReadActiveError::Io(err)),
    };
    let parsed = parse_auth_json(&content)?;
    Ok(parsed.account_id)
}

/// Atomically write `auth_json_text` to `path` with 0600 permissions. Uses
/// a sibling per-call `.tmp.<pid>.<nanos>` file + rename to avoid leaving
/// a partial file if the process crashes mid-write. The unique suffix
/// also prevents two concurrent `activate_codex_credential` calls from
/// interleaving writes into the same `.tmp` file (which would silently
/// produce a blended `auth.json`). On rename failure the `.tmp` file is
/// cleaned up so a permanent error (disk full, permissions) doesn't leave
/// orphan files behind on every retry.
pub fn write_auth_json_atomic(path: &std::path::Path, auth_json_text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let pid = std::process::id();
    let mut tmp = path.to_path_buf();
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("auth.json");
    tmp.set_file_name(format!("{file_name}.tmp.{pid}.{nanos}"));
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&tmp, perms)?;
        }
        f.write_all(auth_json_text.as_bytes())?;
        f.sync_all()?;
    }
    if let Err(err) = std::fs::rename(&tmp, path) {
        global_infra::best_effort!(
            std::fs::remove_file(&tmp),
            "codex_credentials: cleanup auth.json.tmp after rename failure"
        );
        return Err(err);
    }
    Ok(())
}

/// Serialize a stored credential triple back into the `auth.json` shape
/// that `codex login` would have written.
pub fn serialize_auth_json(
    access_token: &str,
    refresh_token: &str,
    id_token: &str,
    account_id: Option<&str>,
    last_refresh: Option<&str>,
) -> serde_json::Result<String> {
    let owned = AuthJsonOwned {
        openai_api_key: None,
        auth_mode: "chatgpt".to_string(),
        tokens: AuthJsonTokensOwned {
            id_token: id_token.to_string(),
            access_token: access_token.to_string(),
            refresh_token: refresh_token.to_string(),
            account_id: account_id.map(|s| s.to_string()),
        },
        last_refresh: last_refresh.map(|s| s.to_string()),
    };
    serde_json::to_string_pretty(&owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jwt(payload: serde_json::Value) -> String {
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload_b = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"signature");
        format!("{header}.{payload_b}.{sig}")
    }

    #[test]
    fn parse_rejects_apikey_mode() {
        let content = r#"{"auth_mode":"apikey","tokens":{"id_token":"a.b.c","access_token":"x","refresh_token":"y"}}"#;
        match parse_auth_json(content).unwrap_err() {
            AuthJsonError::UnsupportedAuthMode(mode) => assert_eq!(mode, "apikey"),
            other => panic!("expected UnsupportedAuthMode, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_missing_tokens() {
        let content = r#"{"auth_mode":"chatgpt"}"#;
        let err = parse_auth_json(content).unwrap_err();
        assert!(matches!(err, AuthJsonError::MissingField("tokens")));
    }

    #[test]
    fn parse_rejects_missing_access_token() {
        let content =
            r#"{"auth_mode":"chatgpt","tokens":{"id_token":"a.b.c","refresh_token":"y"}}"#;
        let err = parse_auth_json(content).unwrap_err();
        assert!(matches!(err, AuthJsonError::MissingField("access_token")));
    }

    #[test]
    fn parse_happy_path() {
        let content = r#"{"auth_mode":"chatgpt","tokens":{"id_token":"a.b.c","access_token":"AT","refresh_token":"RT","account_id":"acct-1"},"last_refresh":"2026-04-25T22:11:34Z"}"#;
        let parsed = parse_auth_json(content).unwrap();
        assert_eq!(parsed.access_token, "AT");
        assert_eq!(parsed.refresh_token, "RT");
        assert_eq!(parsed.id_token, "a.b.c");
        assert_eq!(parsed.account_id.as_deref(), Some("acct-1"));
        assert_eq!(parsed.last_refresh.as_deref(), Some("2026-04-25T22:11:34Z"));
    }

    #[test]
    fn jwt_claims_nested_path() {
        let payload = serde_json::json!({
            "exp": 1777158694i64,
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_account_id": "acct-nested",
            },
        });
        let token = make_jwt(payload);
        let claims = decode_id_token_claims(&token).unwrap();
        assert_eq!(claims.plan_type.as_deref(), Some("pro"));
        assert_eq!(claims.account_id.as_deref(), Some("acct-nested"));
        assert_eq!(claims.exp, Some(1_777_158_694));
    }

    #[test]
    fn jwt_claims_top_level_fallback() {
        let payload = serde_json::json!({
            "exp": 1i64,
            "chatgpt_plan_type": "plus",
            "chatgpt_account_id": "acct-flat",
        });
        let token = make_jwt(payload);
        let claims = decode_id_token_claims(&token).unwrap();
        assert_eq!(claims.plan_type.as_deref(), Some("plus"));
        assert_eq!(claims.account_id.as_deref(), Some("acct-flat"));
    }

    #[test]
    fn jwt_rejects_two_segments() {
        let err = decode_id_token_claims("a.b").unwrap_err();
        assert!(matches!(err, AuthJsonError::MalformedIdToken(_)));
    }

    #[test]
    fn serialize_roundtrip() {
        let serialized = serialize_auth_json(
            "AT",
            "RT",
            "a.b.c",
            Some("acct-1"),
            Some("2026-04-25T22:11:34Z"),
        )
        .unwrap();
        let parsed = parse_auth_json(&serialized).unwrap();
        assert_eq!(parsed.access_token, "AT");
        assert_eq!(parsed.refresh_token, "RT");
        assert_eq!(parsed.id_token, "a.b.c");
        assert_eq!(parsed.account_id.as_deref(), Some("acct-1"));
    }
}
