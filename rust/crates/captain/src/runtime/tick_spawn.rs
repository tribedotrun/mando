//! Worker spawn helper and tick-result defaults.

use std::collections::HashMap;

use crate::{TickMode, TickResult};
use anyhow::Result;
use settings::CaptainWorkflow;
use settings::Config;

pub struct ItemSpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    pub started_at: String,
    /// Worktree-relative path to the plan/brief file, if one was found.
    pub plan: Option<String>,
    pub pr_number: Option<i64>,
}

/// Any picked credential with `last_probed_at` older than this triggers a
/// synchronous pre-spawn probe. Keep this aligned with the scheduled Claude
/// refresh so worker launches do not generate frequent dummy sessions.
const PRE_SPAWN_STALE_SECS: i64 = 3 * 60 * 60;

/// Pick the best credential via a single DB query: not expired, not
/// rate-limited, nearest future weekly reset, then fewest active running
/// sessions and lowest five-hour utilization.
/// Returns `(id, access_token)` or `None` if no credentials are configured.
///
/// The pool is one global load-balancing bucket — every running session on a
/// credential counts toward its tally, whichever caller opened it. There is
/// no per-caller bucket to select.
///
/// When the chosen credential's last probe is older than
/// [`PRE_SPAWN_STALE_SECS`] this also fires a fresh probe. A `Rejected`
/// probe result trips the existing rate-limit cooldown path and the
/// function re-picks. A configured but unavailable pool returns an error.
#[tracing::instrument(skip_all)]
pub async fn pick_credential(pool: &sqlx::SqlitePool) -> Result<Option<(i64, String)>> {
    let pick = pick_credential_probed(pool).await?;
    if pick.is_none() && settings::credentials::has_any(pool).await? {
        anyhow::bail!("No CLI-eligible Claude credential is available");
    }
    Ok(pick)
}

#[tracing::instrument(skip_all)]
pub(super) async fn pick_credential_probed(
    pool: &sqlx::SqlitePool,
) -> Result<Option<(i64, String)>> {
    // Up to 2 pick attempts: if the first pick probes out as `Rejected`,
    // that credential enters cooldown and we try once more.
    let mut any_rejected = false;
    for _ in 0..2 {
        let Some((id, token)) = settings::credentials::pick_for_worker(pool).await? else {
            return Ok(None);
        };
        let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
        let row = match settings::credentials::get_row_by_id(pool, id).await {
            Ok(Some(row)) => row,
            Ok(None) => continue,
            Err(error) => return Err(error),
        };
        let needs_probe = row
            .last_probed_at
            .is_none_or(|last| now_secs - last >= PRE_SPAWN_STALE_SECS)
            || row
                .rate_limit_cooldown_until
                .is_some_and(|until| until > 0 && until <= now_secs);
        if !needs_probe {
            return Ok(Some((id, token)));
        }
        match super::credential_usage_poll::probe_and_persist(pool, &row).await {
            Ok(snapshot)
                if matches!(
                    snapshot.unified_status,
                    settings::usage_probe::RateLimitStatus::Rejected
                ) =>
            {
                any_rejected = true;
                tracing::info!(
                    module = "credentials",
                    credential_id = id,
                    "pre-spawn probe found credential rejected; re-picking"
                );
                // `probe_and_persist` already called `credential_rate_limit::activate`,
                // so the next pick_for_worker excludes this credential.
                continue;
            }
            Ok(_) => return Ok(Some((id, token))),
            Err(settings::usage_probe::ProbeError::RateLimited { .. }) => {
                any_rejected = true;
                continue;
            }
            Err(settings::usage_probe::ProbeError::Unauthorized) => {
                tracing::warn!(
                    module = "credentials",
                    credential_id = id,
                    "pre-spawn probe returned 401; marking expired and re-picking"
                );
                if let Err(e) = settings::credentials::mark_expired(pool, id).await {
                    tracing::warn!(
                        module = "credentials",
                        credential_id = id,
                        error = %e,
                        "failed to mark credential expired after 401"
                    );
                }
                continue;
            }
            Err(e) => {
                tracing::debug!(
                    module = "credentials",
                    credential_id = id,
                    error = %e,
                    "pre-spawn probe transient failure; using stale pick"
                );
                return Ok(Some((id, token)));
            }
        }
    }
    // Loop exhausted without returning: every healthy candidate probed as
    // Rejected. The caller distinguishes an exhausted pool from ambient login.
    if any_rejected {
        tracing::warn!(
            module = "credentials",
            "pick_credential found all candidates rejected; no eligible credential remains"
        );
    }
    Ok(None)
}

#[tracing::instrument(skip_all)]
pub async fn spawn_worker_for_item(
    config: &Config,
    item: &crate::Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<ItemSpawnResult> {
    let (slug, project_config) =
        settings::resolve_project_config(Some(item.project.as_str()), config)
            .ok_or_else(|| anyhow::anyhow!("no project config for '{}'", item.project))?;

    if !item.no_pr && project_config.github_repo.is_none() {
        anyhow::bail!(
            "project '{}' has no githubRepo configured -- cannot process PR-based tasks",
            slug
        );
    }

    let result = super::agent_runtime::spawn_worker(project_config, item, workflow, pool).await?;
    let now = global_types::now_rfc3339();

    // The workbench was created at task-creation time (atomic with the
    // task INSERT). Spawn just resumes a worker against the existing
    // workbench's worktree -- no new workbench row, no slot allocation.
    Ok(ItemSpawnResult {
        session_name: result.session_name,
        session_id: result.session_id,
        branch: result.branch,
        worktree: result.worktree,
        started_at: now,
        plan: result.plan,
        pr_number: result.pr_number,
    })
}

pub(crate) fn default_tick_result() -> TickResult {
    TickResult {
        mode: TickMode::Skipped,
        tick_id: None,
        max_workers: 0,
        active_workers: 0,
        tasks: HashMap::new(),
        alerts: Vec::new(),
        dry_actions: Vec::new(),
        error: None,
        rate_limited: false,
    }
}
