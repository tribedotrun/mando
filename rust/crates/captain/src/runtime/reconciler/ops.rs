use anyhow::Result;
use settings::Config;

use crate::io::ops_log::{self, OpsLog};
use crate::io::task_store::TaskStore;
use crate::service::lifecycle;
use crate::TimelineEventPayload;

/// Typed parameters for a merge reconciliation op, deserialized from ops-log JSON.
pub(super) struct MergeReconcileParams {
    pub pr: String,
    pub repo: String,
    pub item_id: String,
}

/// Typed parameters for an accept reconciliation op, deserialized from ops-log JSON.
pub(super) struct AcceptReconcileParams {
    pub item_id: String,
}

#[tracing::instrument(skip_all)]
pub(super) async fn reconcile_merge(
    log: &mut OpsLog,
    op_id: &str,
    params: MergeReconcileParams,
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let pr = params.pr.as_str();
    let repo = params.repo.as_str();
    let item_id = params.item_id.as_str();

    if pr.is_empty() || repo.is_empty() {
        ops_log::abandon_op(log, op_id, "merge WAL entry has missing pr or repo params");
        return Ok(());
    }

    tracing::info!(module = "reconciler", pr = %pr, item_id = %item_id, "resuming merge");

    match global_github::pr_state(repo, pr).await {
        Ok(global_github::PrState::Merged) => {
            ops_log::mark_step(log, op_id, "squash_merge");
            ops_log::mark_step(log, op_id, "check_merged");
        }
        Ok(global_github::PrState::Open | global_github::PrState::Closed) => {
            ops_log::mark_step(log, op_id, "check_merged");
        }
        Ok(global_github::PrState::Unknown(state)) => {
            tracing::warn!(
                module = "reconciler",
                pr = %pr,
                state = %state,
                "unexpected PR state from GitHub — will retry"
            );
            return Ok(());
        }
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
            let Some(mut task) = store.find_by_id(id).await? else {
                tracing::warn!(
                    module = "reconciler",
                    item_id = id,
                    "task vanished during merge reconciliation"
                );
                return Ok(());
            };
            if let Err(e) = lifecycle::apply_transition(&mut task, crate::ItemStatus::Merged) {
                tracing::warn!(
                    module = "reconciler",
                    item_id = id,
                    error = %e,
                    "illegal merge reconciliation transition — will retry"
                );
                return Ok(());
            }
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(
                    module = "reconciler",
                    item_id = %item_id,
                    error = %e,
                    "failed to update task status to Merged — will retry"
                );
                return Ok(());
            }
            global_infra::best_effort!(
                super::super::timeline_emit::emit(
                    pool,
                    id,
                    "reconciler",
                    &format!("Reconciler confirmed PR {pr} merged on GitHub"),
                    TimelineEventPayload::Merged {
                        pr: pr.to_string(),
                        source: "reconciler".to_string(),
                        accepted_by: "reconciler".to_string(),
                    },
                )
                .await,
                "ops: super::super::timeline_emit::emit( pool, id, 'reconciler', &"
            );
        }
        ops_log::mark_step(log, op_id, "update_task");
    }

    if !ops_log::is_step_done(log, op_id, "post_merge_hook") {
        if let Some((_, project_config)) = settings::resolve_project_config(Some(repo), config) {
            let repo_path = global_infra::paths::expand_tilde(&project_config.path);
            let mut hook_env = std::collections::HashMap::new();
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

#[tracing::instrument(skip_all)]
pub(super) async fn reconcile_accept(
    log: &mut OpsLog,
    op_id: &str,
    params: AcceptReconcileParams,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let item_id = params.item_id.as_str();
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
            let Some(mut task) = store.find_by_id(id).await? else {
                tracing::warn!(
                    module = "reconciler",
                    item_id = id,
                    "task vanished during accept reconciliation"
                );
                return Ok(());
            };
            if let Err(e) = lifecycle::apply_transition(&mut task, crate::ItemStatus::CompletedNoPr)
            {
                tracing::warn!(
                    module = "reconciler",
                    item_id = id,
                    error = %e,
                    "illegal accept reconciliation transition — will retry"
                );
                return Ok(());
            }
            if let Err(e) = store.write_task(&task).await {
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

#[tracing::instrument(skip_all)]
pub(super) async fn reconcile_todo_commit(log: &mut OpsLog, op_id: &str) -> Result<()> {
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

#[tracing::instrument(skip_all)]
pub(super) async fn reconcile_learn(log: &mut OpsLog, op_id: &str) -> Result<()> {
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
