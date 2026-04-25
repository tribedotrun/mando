//! Per-credential rate-limit cooldown — captain-side orchestration.
//!
//! The primitive cooldown math and the single-credential activate/clear
//! writes live in `settings::cc_failover` so scout and captain share one
//! implementation. This module only wires in the captain-specific pieces:
//!
//! - `check_and_activate_from_stream`: scans a worker session's stream
//!   file for a `rate_limit_event rejected` envelope and routes to either
//!   the per-credential cooldown (if the session had a `credential_id`) or
//!   the ambient fallback (for setups without any credential rows).

use sqlx::SqlitePool;

// Re-exports so legacy callsites (notify, credential_usage_poll,
// tick_spawn) don't need to reach into settings. The activate/clear
// behavior is identical; the log `module` field changes to
// `"cc-failover"` so those events cluster together across the scout/
// captain boundary.
pub use settings::cc_failover::{rate_limit_activate as activate, rate_limit_clear as clear};

/// Check a session's stream for rate limit rejection and activate the
/// appropriate cooldown (per-credential or ambient).
///
/// Looks up `credential_id` from the session row to route correctly.
/// Returns `true` if a rejection was found.
#[tracing::instrument(skip_all)]
pub async fn check_and_activate_from_stream(pool: &SqlitePool, session_id: &str) -> bool {
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    match global_claude::has_rate_limit_rejection(&stream_path) {
        Some(rej) => {
            let resets = if rej.resets_at > 0 {
                Some(rej.resets_at)
            } else {
                None
            };
            let rl_type = rej.rate_limit_type.as_deref();
            let cred_id = sessions_db::get_credential_id(pool, session_id)
                .await
                .unwrap_or(None);
            if let Some(cid) = cred_id {
                activate(pool, cid, resets, rl_type).await;
            } else {
                super::ambient_rate_limit::activate(resets);
            }
            true
        }
        None => false,
    }
}
