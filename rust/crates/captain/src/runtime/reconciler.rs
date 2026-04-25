//! Reconciler — resume incomplete operations on startup.

use anyhow::Result;
use serde::Deserialize;
use settings::Config;

use crate::io::ops_log;
use crate::io::task_store::TaskStore;

/// Typed view of a stream `.meta.json` sidecar, scoped to the fields used by
/// cost-backfill reconciliation. Additional fields written by `write_stream_meta`
/// and `update_stream_meta_status` are ignored here.
#[derive(Debug, Deserialize)]
struct StreamMetaCost {
    status: String,
    cost_usd: Option<f64>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

mod ops;

#[tracing::instrument(skip_all)]
pub async fn reconcile_on_startup(config: &Config, pool: &sqlx::SqlitePool) -> Result<()> {
    let orphan_report = reconcile_orphaned_worktrees(config, pool).await;
    if orphan_report.has_problems() {
        tracing::warn!(
            module = "reconciler",
            load_tasks_error = ?orphan_report.load_tasks_error,
            list_worktrees_errors = ?orphan_report.list_worktrees_errors,
            read_dir_error = ?orphan_report.read_dir_error,
            removed = orphan_report.removed.len(),
            remove_errors = ?orphan_report.remove_errors,
            "orphaned-worktree reconcile completed with problems"
        );
    }
    reconcile_worker_timeouts(pool).await;
    reconcile_session_costs(pool).await;
    // Resolve any cc_sessions still marked running from the previous daemon
    // instance. Orphan subprocesses were already killed in
    // `pid_registry::cleanup_on_startup`; this step settles DB state so the
    // captain tick can salvage usable stream results immediately instead
    // of waiting for the wall-clock clarifier/review/merge timeouts.
    // Propagate: a DB query failure here means we would boot with stale
    // sessions stuck in running. The gateway's `MANDO_UNSAFE_START` gate
    // is the authoritative opt-out for operators who want to boot anyway.
    super::startup_session_reconcile::reconcile_startup_sessions(pool).await?;
    // Unstick any task stranded in `Clarifying` from a prior daemon run.
    // Replaces what the deleted `dispatch_reclarify` safety net used to
    // catch during normal ticks: with a single writer for follow-up
    // clarifier work and the HTTP path no longer nulling the session id,
    // the only way a task lands here is a daemon crash mid-inline-call.
    super::startup_session_reconcile::reconcile_stranded_clarifying_tasks(pool).await;
    super::dispatch_planning::reconcile_orphaned_planning(pool).await;

    let log_path = ops_log::ops_log_path();
    let mut log = ops_log::load_ops_log(&log_path);

    ops_log::prune_stale(&mut log, ops_log::STALE_AGE_SECS);
    ops_log::save_ops_log(&log, &log_path)?;

    let incomplete = ops_log::incomplete_ops(&log);
    if incomplete.is_empty() {
        tracing::debug!(module = "reconciler", "no incomplete operations to resume");
        return Ok(());
    }

    tracing::info!(
        module = "reconciler",
        count = incomplete.len(),
        "found incomplete operations, resuming"
    );

    // Collect into owned tuples — one pass instead of three separate Vecs.
    let entries: Vec<(String, String, serde_json::Value)> = incomplete
        .iter()
        .map(|e| (e.op_id.clone(), e.op_type.clone(), e.params.clone()))
        .collect();

    for (op_id, op_type, params) in &entries {
        match op_type.as_str() {
            "merge" => {
                let typed = ops::MergeReconcileParams {
                    pr: params["pr"].as_str().unwrap_or("").to_string(),
                    repo: params["repo"].as_str().unwrap_or("").to_string(),
                    item_id: params["item_id"].as_str().unwrap_or("").to_string(),
                };
                ops::reconcile_merge(&mut log, op_id, typed, config, pool).await?;
            }
            "accept" => {
                let typed = ops::AcceptReconcileParams {
                    item_id: params["item_id"].as_str().unwrap_or("").to_string(),
                };
                ops::reconcile_accept(&mut log, op_id, typed, pool).await?;
            }
            "todo_commit" => {
                ops::reconcile_todo_commit(&mut log, op_id).await?;
            }
            "learn" => {
                ops::reconcile_learn(&mut log, op_id).await?;
            }
            _ => {
                tracing::warn!(
                    module = "reconciler",
                    op_type = %op_type,
                    op_id = %op_id,
                    "unknown op_type, skipping"
                );
                ops_log::complete_op(&mut log, op_id);
            }
        }
    }

    ops_log::save_ops_log(&log, &log_path)?;
    tracing::info!(module = "reconciler", "reconciliation complete");
    Ok(())
}

/// Reset `worker_started_at` for all InProgress workers so daemon downtime
/// (crashes, restarts, laptop sleep) does not count toward the timeout budget.
async fn reconcile_worker_timeouts(pool: &sqlx::SqlitePool) {
    let store = TaskStore::new(pool.clone());
    let tasks = match store.load_all().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(module = "reconciler", error = %e,
                "failed to load tasks — skipping worker timeout reconciliation");
            return;
        }
    };
    let now = global_types::now_rfc3339();
    let mut count = 0u32;
    for task in &tasks {
        if task.status == crate::ItemStatus::InProgress && task.worker_started_at.is_some() {
            if let Err(e) = store
                .update(task.id, |t| {
                    t.worker_started_at = Some(now.clone());
                })
                .await
            {
                tracing::warn!(module = "reconciler", task_id = task.id, error = %e,
                    "failed to reset worker_started_at on startup");
            } else {
                count += 1;
            }
        }
    }
    if count > 0 {
        tracing::info!(
            module = "reconciler",
            count,
            "reset worker timeout clocks on startup"
        );
    }
}

/// Backfill cost/duration for sessions that were interrupted before
/// `log_cc_result` ran. Reads the stream `.meta.json` sidecar for each
/// session with NULL cost and applies any cost/duration found there.
/// The `cost_usd IS NULL` filter in `list_sessions_missing_cost` ensures
/// each session is reconciled at most once per startup.
async fn reconcile_session_costs(pool: &sqlx::SqlitePool) {
    let sessions = match sessions_db::list_sessions_missing_cost(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(module = "reconciler", error = %e,
                    "failed to load sessions — skipping cost reconciliation");
            return;
        }
    };

    let mut count = 0u32;
    for session in &sessions {
        let meta_path = global_infra::paths::stream_meta_path_for_session(&session.session_id);
        let data = match std::fs::read_to_string(&meta_path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!(module = "reconciler", session_id = %session.session_id,
                    error = %e, "failed to read stream meta");
                continue;
            }
        };
        let meta: StreamMetaCost = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(module = "reconciler", session_id = %session.session_id,
                    error = %e, "corrupt stream meta sidecar");
                continue;
            }
        };
        // Meta with `status == "done"` AND a recorded cost is the happy
        // path — use it verbatim. Meta with `status != "done"` or missing
        // cost is the abnormal-exit case (watchdog abort, process kill):
        // fall back to aggregating per-message usage from the stream file
        // and estimating cost from the per-model pricing table so the DB
        // row stops silently recording NULL for real token spend.
        let cost = match meta.cost_usd {
            Some(c) if c > 0.0 && meta.status == "done" => Some(c),
            _ => {
                let stream_path = global_infra::paths::stream_path_for_session(&session.session_id);
                global_claude::session_cost_or_estimate(&stream_path).total_cost_usd
            }
        };
        if cost.is_none() || cost.unwrap_or(0.0) <= 0.0 {
            continue;
        }
        // Compute duration from started_at / finished_at in the meta.
        let duration_ms = (|| {
            let started = time::OffsetDateTime::parse(
                meta.started_at.as_deref()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?;
            let finished = time::OffsetDateTime::parse(
                meta.finished_at.as_deref()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?;
            let dur = finished - started;
            Some((dur.whole_milliseconds().max(0)) as i64)
        })();

        if let Err(e) = sessions_db::update_session_status_with_cost(
            pool,
            &session.session_id,
            global_types::SessionStatus::Stopped,
            cost,
            duration_ms,
            None,
        )
        .await
        {
            tracing::warn!(module = "reconciler", session_id = %session.session_id, error = %e,
                "failed to backfill session cost");
        } else {
            count += 1;
        }
    }
    if count > 0 {
        tracing::info!(
            module = "reconciler",
            count,
            "backfilled session costs from stream meta"
        );
    }
}

use super::reconciler_orphans::reconcile_orphaned_worktrees;
