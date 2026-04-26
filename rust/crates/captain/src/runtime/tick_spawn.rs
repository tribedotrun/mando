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
/// synchronous pre-spawn probe. Catches the case where a credential sits at
/// 79% utilization between scheduled poll ticks and a new worker would
/// otherwise sail into the 5h wall a few minutes in.
const PRE_SPAWN_STALE_SECS: i64 = 300;

/// Pick the best credential via a single DB query: not expired, not
/// rate-limited, fewest active running sessions, tiebreak on lowest
/// five-hour utilization.
/// Returns `(id, access_token)` or `None` if no credentials are configured.
///
/// `caller_filter` narrows which running sessions count toward load
/// balancing. Pass `Some("worker")` for worker spawns so only other
/// workers influence the pick. Pass `None` to count all sessions.
///
/// When the chosen credential's last probe is older than
/// [`PRE_SPAWN_STALE_SECS`] this also fires a fresh probe. A `Rejected`
/// probe result trips the existing rate-limit cooldown path and the
/// function re-picks, returning `None` if no healthy credential remains.
#[tracing::instrument(skip_all)]
pub async fn pick_credential(
    pool: &sqlx::SqlitePool,
    caller_filter: Option<&str>,
) -> Option<(i64, String)> {
    // Up to 2 pick attempts: if the first pick probes out as `Rejected`,
    // that credential enters cooldown and we try once more.
    let mut any_rejected = false;
    for _ in 0..2 {
        let picked = match settings::credentials::pick_for_worker(pool, caller_filter).await {
            Ok(pick) => pick,
            Err(e) => {
                tracing::warn!(
                    module = "credentials",
                    error = %e,
                    "failed to pick credential"
                );
                return None;
            }
        };
        let (id, token) = picked?;
        let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
        let row = match settings::credentials::get_row_by_id(pool, id).await {
            Ok(Some(row)) => row,
            _ => return Some((id, token)),
        };
        let needs_probe = row
            .last_probed_at
            .is_none_or(|last| now_secs - last > PRE_SPAWN_STALE_SECS);
        if !needs_probe {
            return Some((id, token));
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
            Ok(_) => return Some((id, token)),
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
                return Some((id, token));
            }
        }
    }
    // Loop exhausted without returning: every healthy candidate probed as
    // Rejected. Surface it so operators can tell "all rate-limited" from
    // "no credentials configured" (both currently return None, which the
    // caller treats as "fall back to ambient login").
    if any_rejected {
        tracing::warn!(
            module = "credentials",
            "pick_credential found all candidates rejected; falling back to ambient login"
        );
    }
    None
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

    let claude_path = global_claude::resolve_claude_binary();
    if !claude_path.exists() && claude_path.to_str() == Some("claude") {
        let which = tokio::process::Command::new("which")
            .arg("claude")
            .output()
            .await;
        match which {
            Ok(out) if out.status.success() => {}
            _ => {
                anyhow::bail!(
                    "claude binary not found (checked {:?} and PATH)",
                    claude_path
                );
            }
        }
    }

    // Pick credential for multi-account load balancing.
    // Workers dominate token spend, so balance on worker sessions only.
    let credential = pick_credential(pool, Some("worker")).await;
    let worker_cred = credential
        .as_ref()
        .map(|c| super::spawner::WorkerCredential {
            id: c.0,
            token: &c.1,
        });

    let result = super::spawner::spawn_worker(
        item,
        slug,
        project_config,
        &config.captain,
        workflow,
        pool,
        worker_cred.as_ref(),
    )
    .await?;
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
