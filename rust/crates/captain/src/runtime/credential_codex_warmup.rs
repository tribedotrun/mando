//! Automatic Codex usage warm-up, driven by the credential usage poller.
//!
//! After each successful Codex probe the poller asks
//! [`settings::codex_warmup_due`] whether the credential is idle with no
//! reset clock running. When it is, one throwaway `codex exec` prompt runs
//! through `SettingsRuntime::warm_codex_credential`, which starts the
//! rolling 5h/7d windows so they are already part-way to reset by the time
//! the credential is picked for real work. The `codex_warmup_at` stamp on
//! the row keeps this to one warm-up per window; the in-memory attempt map
//! here only rate-limits retries after a failed run.

use std::collections::HashMap;

use tracing::{info, warn};

use settings::credentials::CredentialRow;
use settings::usage_probe::UsageSnapshot;
use settings::SettingsRuntime;

/// Do not retry a failed warm-up on the same credential sooner than this.
/// Success is tracked on the row itself (`codex_warmup_at`), so this only
/// throttles failures (missing codex binary, model rejected, timeouts).
const FAILED_WARMUP_RETRY_SECS: i64 = 60 * 60;

/// Unix seconds of the last warm-up attempt per credential, successful or
/// not. Lives only as long as the poller task.
#[derive(Default)]
pub(super) struct WarmupAttempts {
    last: HashMap<i64, i64>,
}

impl WarmupAttempts {
    pub(super) fn retain_ids(&mut self, keep: impl Fn(i64) -> bool) {
        self.last.retain(|id, _| keep(*id));
    }

    fn recently_attempted(&self, id: i64, now_secs: i64) -> bool {
        self.last
            .get(&id)
            .is_some_and(|last| now_secs - last < FAILED_WARMUP_RETRY_SECS)
    }
}

/// Warm `row` if its freshly probed `snapshot` says it is due. Outcomes are
/// logged; nothing here fails the poll tick.
#[tracing::instrument(skip_all, fields(credential_id = row.id, label = %row.label))]
pub(super) async fn maybe_warm(
    settings: &SettingsRuntime,
    row: &CredentialRow,
    snapshot: &UsageSnapshot,
    attempts: &mut WarmupAttempts,
    now_secs: i64,
) {
    if !settings::codex_warmup_due(row, snapshot, now_secs) {
        return;
    }
    if attempts.recently_attempted(row.id, now_secs) {
        return;
    }
    attempts.last.insert(row.id, now_secs);
    info!(
        module = "captain",
        credential_id = row.id,
        label = %row.label,
        "Codex credential idle with no usage clock running; starting warm-up prompt"
    );
    match settings.warm_codex_credential(row.id).await {
        Ok(report) => info!(
            module = "captain",
            credential_id = row.id,
            label = %row.label,
            model = report.model.as_deref().unwrap_or("default"),
            elapsed_ms = report.elapsed_ms,
            tokens_rotated = report.tokens_rotated,
            "Codex warm-up started the credential's usage clock"
        ),
        Err(e) => warn!(
            module = "captain",
            credential_id = row.id,
            label = %row.label,
            error = %e,
            retry_after_secs = FAILED_WARMUP_RETRY_SECS,
            "Codex warm-up failed"
        ),
    }
}
