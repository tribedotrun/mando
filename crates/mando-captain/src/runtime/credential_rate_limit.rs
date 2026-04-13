//! Per-credential rate-limit cooldown.
//!
//! When a session using a specific credential hits a rate limit, that
//! credential is marked with `rate_limit_cooldown_until` in the DB.
//! `pick_for_worker` already filters these out, so healthy credentials
//! keep being selected while rate-limited ones cool down.

use sqlx::SqlitePool;

/// Mark a credential as rate-limited until `resets_at` (unix seconds).
/// If `resets_at` is unknown or in the past, uses a 10-minute default.
pub async fn activate(pool: &SqlitePool, credential_id: i64, resets_at: Option<u64>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let until = match resets_at {
        Some(ts) if ts > now => ts + 30, // buffer
        _ => now + 600,                  // 10-minute default
    };

    match mando_db::queries::credentials::set_rate_limit_cooldown(pool, credential_id, until as i64)
        .await
    {
        Ok(_) => {
            tracing::warn!(
                module = "captain",
                credential_id,
                until_epoch = until,
                "credential rate-limited"
            );
        }
        Err(e) => {
            tracing::error!(
                module = "captain",
                credential_id,
                error = %e,
                "failed to set credential rate limit cooldown"
            );
        }
    }
}

/// Clear rate-limit cooldown for a credential (recovery).
pub async fn clear(pool: &SqlitePool, credential_id: i64) {
    match mando_db::queries::credentials::set_rate_limit_cooldown(pool, credential_id, 0).await {
        Ok(_) => {
            tracing::info!(
                module = "captain",
                credential_id,
                "credential rate limit cleared (recovery)"
            );
        }
        Err(e) => {
            tracing::error!(
                module = "captain",
                credential_id,
                error = %e,
                "failed to clear credential rate limit cooldown"
            );
        }
    }
}

/// Check a session's stream for rate limit rejection and activate the
/// appropriate cooldown (per-credential or ambient).
///
/// Looks up `credential_id` from the session row to route correctly.
/// Returns `true` if a rejection was found.
pub async fn check_and_activate_from_stream(pool: &SqlitePool, session_id: &str) -> bool {
    let stream_path = mando_config::stream_path_for_session(session_id);
    match mando_cc::has_rate_limit_rejection(&stream_path) {
        Some(resets_at) => {
            let resets = if resets_at > 0 { Some(resets_at) } else { None };
            let cred_id = mando_db::queries::sessions::get_credential_id(pool, session_id)
                .await
                .unwrap_or(None);
            if let Some(cid) = cred_id {
                activate(pool, cid, resets).await;
            } else {
                super::ambient_rate_limit::activate(resets);
            }
            true
        }
        None => false,
    }
}
