//! Add-time persistence helpers (Fix 1 / Fix 4). Split out of
//! `codex_credentials_runtime.rs` to keep that file under the file length
//! budget.

use global_types::RateLimitStatus;

use crate::io::cc_failover;
use crate::io::codex_credentials;
use crate::io::codex_probe::CodexProbeOutcome;
use crate::io::credentials;

use super::codex_credentials_runtime::CodexCredentialError;

/// Insert-or-replace helper shared by every `force_refresh_for_add` outcome
/// branch that needs to persist a Codex credential row. `token_updated_at`
/// is caller-supplied (Fix 4): either "now" (rotation succeeded) or the
/// pasted session's own age (rotation was skipped and the tokens are
/// unvalidated) — see [`codex_credentials::insert_codex`] /
/// [`codex_credentials::replace_codex`].
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(pool, access_token, refresh_token, id_token))]
pub(super) async fn persist_codex_row(
    pool: &sqlx::SqlitePool,
    existing_id: Option<i64>,
    label: &str,
    access_token: &str,
    refresh_token: &str,
    id_token: Option<&str>,
    account_id: &str,
    plan_type: Option<&str>,
    expires_at: Option<i64>,
    token_updated_at: i64,
) -> Result<i64, CodexCredentialError> {
    if let Some(existing_id) = existing_id {
        let updated = codex_credentials::replace_codex(
            pool,
            existing_id,
            label,
            access_token,
            refresh_token,
            id_token,
            account_id,
            plan_type,
            expires_at,
            token_updated_at,
        )
        .await?;
        if !updated {
            return Err(CodexCredentialError::NotFound(existing_id));
        }
        Ok(existing_id)
    } else {
        codex_credentials::insert_codex(
            pool,
            label,
            access_token,
            refresh_token,
            id_token,
            account_id,
            plan_type,
            expires_at,
            token_updated_at,
        )
        .await
        .map_err(Into::into)
    }
}

/// Persist the side effects of a successful add-time usage probe: the
/// usage snapshot, plan/credits metadata, and (if the probe came back
/// rejected) a rate-limit cooldown. Best-effort — none of these failing
/// should fail the add itself, since the credential row is already safely
/// persisted (Fix 1) by the time this runs.
#[tracing::instrument(skip(pool, outcome))]
pub(super) async fn persist_codex_probe_side_effects(
    pool: &sqlx::SqlitePool,
    id: i64,
    outcome: &CodexProbeOutcome,
    plan_type: Option<&str>,
) {
    global_infra::best_effort!(
        credentials::set_usage_snapshot(pool, id, &outcome.snapshot).await,
        "codex_add_persist: set_usage_snapshot on add"
    );
    global_infra::best_effort!(
        codex_credentials::update_codex_plan_and_credits(
            pool,
            id,
            plan_type,
            outcome.credits_balance.as_deref(),
            outcome.credits_unlimited,
        )
        .await,
        "codex_add_persist: update_codex_plan_and_credits on add"
    );
    if matches!(outcome.snapshot.unified_status, RateLimitStatus::Rejected) {
        let reset_at = binding_reset_at(&outcome.snapshot).max(0) as u64;
        let until = cc_failover::compute_cooldown_until(
            time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64,
            Some(reset_at),
            outcome.snapshot.representative_claim.as_deref(),
        );
        global_infra::best_effort!(
            credentials::set_rate_limit_cooldown(pool, id, until as i64).await,
            "codex_add_persist: set cooldown on rejected add probe"
        );
    }
}

fn binding_reset_at(snapshot: &crate::io::usage_probe::UsageSnapshot) -> i64 {
    match snapshot.representative_claim.as_deref() {
        Some("five_hour") => snapshot.five_hour.reset_at,
        Some(s) if s.starts_with("seven_day") => snapshot.seven_day.reset_at,
        _ => snapshot.five_hour.reset_at.max(snapshot.seven_day.reset_at),
    }
}
