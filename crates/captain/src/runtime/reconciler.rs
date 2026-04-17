//! Reconciler — resume incomplete operations on startup.

use anyhow::Result;
use settings::config::settings::Config;

use crate::io::ops_log::{self, OpsLog};
use crate::io::task_store::TaskStore;

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
                reconcile_merge(&mut log, op_id, params, config, pool).await?;
            }
            "accept" => {
                reconcile_accept(&mut log, op_id, params, pool).await?;
            }
            "todo_commit" => {
                reconcile_todo_commit(&mut log, op_id).await?;
            }
            "learn" => {
                reconcile_learn(&mut log, op_id).await?;
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
    let sessions = match sessions::io::queries::list_sessions_missing_cost(pool).await {
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
        let val: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(module = "reconciler", session_id = %session.session_id,
                    error = %e, "corrupt stream meta sidecar");
                continue;
            }
        };
        let status = val["status"].as_str().unwrap_or("");
        if status != "done" {
            continue;
        }
        let cost = val["cost_usd"].as_f64();
        if cost.is_none() {
            continue;
        }
        // Compute duration from started_at / finished_at in the meta.
        let duration_ms = (|| {
            let started = time::OffsetDateTime::parse(
                val["started_at"].as_str()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?;
            let finished = time::OffsetDateTime::parse(
                val["finished_at"].as_str()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?;
            let dur = finished - started;
            Some((dur.whole_milliseconds().max(0)) as i64)
        })();

        if let Err(e) = sessions::io::queries::update_session_status_with_cost(
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

async fn reconcile_merge(
    log: &mut OpsLog,
    op_id: &str,
    params: &serde_json::Value,
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let pr = params["pr"].as_str().unwrap_or("");
    let repo = params["repo"].as_str().unwrap_or("");
    let item_id = params["item_id"].as_str().unwrap_or("");

    if pr.is_empty() || repo.is_empty() {
        ops_log::abandon_op(log, op_id, "merge WAL entry has missing pr or repo params");
        return Ok(());
    }

    tracing::info!(module = "reconciler", pr = %pr, item_id = %item_id, "resuming merge");

    // Always re-check GitHub state (even if check_merged was set on a previous run)
    // to avoid stale data — the PR may have been merged between restarts.
    {
        let output = tokio::process::Command::new("gh")
            .args(["pr", "view", pr, "--json", "state", "--repo", repo])
            .output()
            .await;

        match output {
            Ok(output) => match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                Ok(json) => match json["state"].as_str() {
                    Some("MERGED") => {
                        ops_log::mark_step(log, op_id, "squash_merge");
                        ops_log::mark_step(log, op_id, "check_merged");
                    }
                    Some("OPEN") | Some("CLOSED") => {
                        ops_log::mark_step(log, op_id, "check_merged");
                    }
                    other => {
                        tracing::warn!(
                            module = "reconciler",
                            pr = %pr,
                            state = ?other,
                            "unexpected PR state from GitHub — will retry"
                        );
                        return Ok(());
                    }
                },
                Err(e) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        module = "reconciler",
                        pr = %pr,
                        error = %e,
                        stderr = %stderr,
                        "gh pr view returned non-JSON — will retry"
                    );
                    return Ok(());
                }
            },
            Err(e) => {
                tracing::warn!(
                    module = "reconciler",
                    pr = %pr,
                    error = %e,
                    "gh pr view failed — will retry check_merged on next reconciliation"
                );
                return Ok(());
            }
        }
    }

    // PR was not actually merged on GitHub — abandon this WAL entry.
    if !ops_log::is_step_done(log, op_id, "squash_merge") {
        tracing::warn!(
            module = "reconciler",
            pr = %pr,
            item_id = %item_id,
            "PR is not merged on GitHub — abandoning stale merge WAL entry"
        );
        ops_log::abandon_op(log, op_id, &format!("PR {pr} not merged on GitHub"));
        return Ok(());
    }

    if !ops_log::is_step_done(log, op_id, "update_task") {
        let store = TaskStore::new(pool.clone());
        let item_id_num: i64 = match item_id.parse() {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(
                    module = "reconciler",
                    op_id = %op_id,
                    raw = item_id,
                    error = %e,
                    "corrupt item_id in merge ops log; leaving WAL entry for manual review, not completing op"
                );
                // Do NOT coerce to 0 and complete the op. Leave the WAL entry
                // alone so the next reconcile pass (or an operator) can see
                // the corruption and fix it explicitly.
                return Ok(());
            }
        };
        let match_id = if item_id_num > 0 {
            store
                .find_by_id(item_id_num)
                .await
                .ok()
                .flatten()
                .map(|_| item_id_num)
        } else {
            None
        };
        let match_id = match match_id {
            Some(id) => Some(id),
            None => match store.load_all().await {
                Ok(tasks) => {
                    let pr_num = match crate::parse_pr_number(pr) {
                        Some(n) => n,
                        None => {
                            tracing::warn!(module = "reconciler", pr = %pr, "unparseable PR ref in merge WAL — abandoning");
                            ops_log::abandon_op(log, op_id, "unparseable PR ref");
                            return Ok(());
                        }
                    };
                    tasks
                        .iter()
                        .find(|t| t.pr_number == Some(pr_num))
                        .map(|t| t.id)
                }

                Err(e) => {
                    tracing::warn!(module = "reconciler", error = %e, "failed to load tasks for PR lookup");
                    None
                }
            },
        };
        if let Some(id) = match_id {
            if let Err(e) = store
                .update(id, |t| {
                    t.status = crate::ItemStatus::Merged;
                })
                .await
            {
                tracing::warn!(
                    module = "reconciler",
                    item_id = %item_id,
                    error = %e,
                    "failed to update task status to Merged — will retry"
                );
                return Ok(());
            }
            let _ = super::timeline_emit::emit(
                pool,
                id,
                crate::TimelineEventType::Merged,
                "reconciler",
                &format!("Reconciler confirmed PR {pr} merged on GitHub"),
                serde_json::json!({ "pr": pr, "source": "reconciler" }),
            )
            .await;
        }
        ops_log::mark_step(log, op_id, "update_task");
    }

    if !ops_log::is_step_done(log, op_id, "post_merge_hook") {
        if let Some((_, project_config)) =
            settings::config::resolve_project_config(Some(repo), config)
        {
            let repo_path = global_infra::paths::expand_tilde(&project_config.path);
            let mut hook_env = std::collections::HashMap::new();
            // Resolve the task's worktree path using the item_id from the WAL entry.
            // This is scoped to the exact task, avoiding PR number collisions across repos.
            let store = TaskStore::new(pool.clone());
            match item_id.parse::<i64>() {
                Ok(id) if id > 0 => match store.find_by_id(id).await {
                    Ok(Some(task)) => {
                        if let Some(ref wt) = task.worktree {
                            hook_env.insert("MANDO_WORKTREE".to_string(), wt.clone());
                        } else {
                            tracing::debug!(module = "reconciler", pr = %pr, item_id = %item_id, "task has no worktree field");
                        }
                    }
                    Ok(None) => {
                        tracing::debug!(module = "reconciler", pr = %pr, item_id = %item_id, "task not found for worktree resolution");
                    }
                    Err(e) => {
                        tracing::warn!(module = "reconciler", pr = %pr, item_id = %item_id, error = %e, "failed to load task for worktree resolution");
                    }
                },
                _ => {
                    tracing::debug!(module = "reconciler", pr = %pr, item_id = %item_id, "invalid item_id, skipping worktree resolution");
                }
            }
            if let Err(e) =
                crate::io::hooks::post_merge(&project_config.hooks, &repo_path, &hook_env).await
            {
                tracing::warn!(
                    module = "reconciler",
                    pr = %pr,
                    error = %e,
                    "post-merge hook failed"
                );
            }
        }
        ops_log::mark_step(log, op_id, "post_merge_hook");
    }

    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_accept(
    log: &mut OpsLog,
    op_id: &str,
    params: &serde_json::Value,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let item_id = params["item_id"].as_str().unwrap_or("");
    tracing::info!(module = "reconciler", item_id = %item_id, "resuming accept");

    if !ops_log::is_step_done(log, op_id, "update_task") {
        let store = TaskStore::new(pool.clone());
        let id: i64 = match item_id.parse() {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(
                    module = "reconciler",
                    op_id = %op_id,
                    raw = item_id,
                    error = %e,
                    "corrupt item_id in accept ops log; leaving WAL entry for manual review"
                );
                return Ok(());
            }
        };
        if id > 0 {
            if let Err(e) = store
                .update(id, |t| {
                    t.status = crate::ItemStatus::CompletedNoPr;
                })
                .await
            {
                tracing::warn!(
                    module = "reconciler",
                    item_id = %item_id,
                    error = %e,
                    "failed to update task status to CompletedNoPr — will retry"
                );
                return Ok(());
            }
        }
        ops_log::mark_step(log, op_id, "update_task");
    }

    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_todo_commit(log: &mut OpsLog, op_id: &str) -> Result<()> {
    // No recovery logic is required for todo_commit: the task and commit
    // write happen inline before the op is logged, so a resume just finalizes
    // the WAL entry. Emit a structured event so unexpected drift is visible.
    tracing::info!(
        module = "reconciler",
        op_id = %op_id,
        event = "reconcile_no_recovery_required",
        op_type = "todo_commit",
        "todo_commit reconcile: no recovery required, completing op"
    );
    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_learn(log: &mut OpsLog, op_id: &str) -> Result<()> {
    // No recovery logic is required for learn: the persisted knowledge note is
    // the source of truth, so a resume just finalizes the WAL entry.
    tracing::info!(
        module = "reconciler",
        op_id = %op_id,
        event = "reconcile_no_recovery_required",
        op_type = "learn",
        "learn reconcile: no recovery required, completing op"
    );
    ops_log::complete_op(log, op_id);
    Ok(())
}
