//! Per-tick rate-limit cooldown detection.
//!
//! Two independent gates: when credentials are configured, the tick is
//! cooled-down iff every credential is rate-limited (i.e.
//! `pick_for_worker` returns `None`). When no credentials are
//! configured, fall back to the ambient (host login) cooldown.

use sqlx::SqlitePool;

#[tracing::instrument(skip_all)]
pub(crate) async fn detect_cooldown(pool: &SqlitePool) -> bool {
    let has_credentials = settings::credentials::has_any(pool).await.unwrap_or(false);
    let rate_limited = if has_credentials {
        settings::credentials::pick_for_worker(pool, None)
            .await
            .unwrap_or(None)
            .is_none()
    } else {
        super::ambient_rate_limit::is_active()
    };
    if rate_limited {
        let remaining = if has_credentials {
            0
        } else {
            super::ambient_rate_limit::remaining_secs()
        };
        tracing::warn!(
            module = "captain",
            remaining_s = remaining,
            "rate limit cooldown active — CC session spawning suppressed"
        );
    }
    rate_limited
}
