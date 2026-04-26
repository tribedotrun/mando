//! Re-dispatch pass — spawn workers for items that became Queued during the
//! same tick (e.g. after clarification completes).

use std::collections::{HashMap, HashSet};

use crate::{ItemStatus, Task, TimelineEventPayload};
use settings::CaptainWorkflow;
use settings::Config;

use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;
use crate::service::{dispatch_logic, lifecycle};

/// Shared escalation threshold for both dispatch paths
/// (`dispatch_phase.rs` and `dispatch_redispatch.rs`). Keep them in lockstep so
/// `spawn_fail` escalates after the same count regardless of which pass
/// incremented `spawn_fail_count`.
pub(super) const MAX_SPAWN_FAILS: i64 = 3;

/// Dispatch newly-Queued items that were clarified in this tick.
///
/// `already_dispatched` contains IDs of items that were already Queued at the
/// start of the tick and were dispatched (or attempted) in the first pass.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(crate) async fn redispatch_newly_queued(
    items: &mut [Task],
    config: &Config,
    active_workers: &mut usize,
    max_workers: usize,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    alerts: &mut Vec<String>,
    resource_limits: &HashMap<String, usize>,
    resource_counts: &mut HashMap<String, usize>,
    pool: &sqlx::SqlitePool,
    already_dispatched: &HashSet<i64>,
) {
    let newly_ready = dispatch_logic::dispatchable_items(items);
    for idx in newly_ready {
        if items[idx].status != ItemStatus::Queued {
            continue;
        }
        // Skip items that were already Queued at tick start.
        if already_dispatched.contains(&items[idx].id) {
            continue;
        }
        let item = &items[idx];
        let decision = dispatch_logic::check_dispatch(
            item,
            *active_workers,
            max_workers,
            resource_limits,
            resource_counts,
        );
        match decision {
            dispatch_logic::DispatchDecision::Spawn => {
                if dry_run {
                    dry_actions.push(format!(
                        "would spawn worker for '{}'",
                        truncate_utf8(&item.title, 60)
                    ));
                    *active_workers += 1;
                    let resource = item
                        .resource
                        .as_deref()
                        .unwrap_or(dispatch_logic::DEFAULT_RESOURCE)
                        .to_string();
                    *resource_counts.entry(resource).or_insert(0) += 1;
                } else {
                    items[idx].worker_seq += 1;
                    match super::tick::spawn_worker_for_item(config, &items[idx], workflow, pool)
                        .await
                    {
                        Ok(spawn_result) => {
                            let item = &mut items[idx];
                            if let Err(e) =
                                lifecycle::apply_transition(item, ItemStatus::InProgress)
                            {
                                tracing::error!(
                                    module = "captain",
                                    item_id = item.id,
                                    error = %e,
                                    "illegal redispatch transition"
                                );
                                item.worker_seq = item.worker_seq.saturating_sub(1);
                                crate::io::session_terminate::terminate_session(
                                    pool,
                                    &spawn_result.session_id,
                                    global_types::SessionStatus::Failed,
                                    None,
                                )
                                .await;
                                continue;
                            }
                            item.worker = Some(spawn_result.session_name.clone());
                            item.branch = Some(spawn_result.branch);
                            item.worktree = Some(spawn_result.worktree.clone());
                            item.worker_started_at = Some(spawn_result.started_at);
                            item.session_ids.worker = Some(spawn_result.session_id);
                            item.plan = spawn_result.plan;
                            item.pr_number = spawn_result.pr_number;
                            item.spawn_fail_count = 0;
                            *active_workers += 1;
                            let resource = item
                                .resource
                                .as_deref()
                                .unwrap_or(dispatch_logic::DEFAULT_RESOURCE)
                                .to_string();
                            *resource_counts.entry(resource).or_insert(0) += 1;

                            if let Err(e) =
                                crate::io::queries::tasks::persist_spawn(pool, item).await
                            {
                                tracing::error!(module = "captain", id = item.id, error = %e,
                                    "failed to persist spawn — killing orphan worker");
                                if let Some(ref cc_sid) = item.session_ids.worker {
                                    crate::io::session_terminate::terminate_session(
                                        pool,
                                        cc_sid,
                                        global_types::SessionStatus::Failed,
                                        None,
                                    )
                                    .await;
                                }
                                super::revert_to_queued(item);
                                *active_workers -= 1;
                                let resource = item
                                    .resource
                                    .as_deref()
                                    .unwrap_or(dispatch_logic::DEFAULT_RESOURCE)
                                    .to_string();
                                if let Some(c) = resource_counts.get_mut(&resource) {
                                    *c = c.saturating_sub(1);
                                }
                                continue;
                            }

                            global_infra::best_effort!(super::timeline_emit::emit_for_task(
                                item,
                                &format!("Spawned {}", spawn_result.session_name),
                                crate::TimelineEventPayload::WorkerSpawned {
                                    worker: spawn_result.session_name.clone(),
                                    session_id: item.session_ids.worker.clone().unwrap_or_default(),
                                },
                                pool,
                            )
                            .await, "dispatch_redispatch: super::timeline_emit::emit_for_task( item, &format!('Spawned");
                            let msg = format!(
                                "\u{1f477} Spawned \u{2192} {}: <b>{}</b>",
                                spawn_result.session_name,
                                global_infra::html::escape_html(&item.title),
                            );
                            notifier.normal(&msg).await;
                        }
                        Err(e) => {
                            let item = &mut items[idx];
                            item.worker_seq -= 1;
                            let count = item.spawn_fail_count + 1;
                            item.spawn_fail_count = count;

                            if count >= MAX_SPAWN_FAILS {
                                super::action_contract::reset_review_retry(
                                    item,
                                    crate::ReviewTrigger::SpawnFail,
                                );
                                let msg = format!(
                                    "Spawn failed {} times for '{}', escalated to captain review: {}",
                                    count,
                                    truncate_utf8(&item.title, 60),
                                    e
                                );
                                tracing::error!(module = "captain", error = %msg, "spawn permanently failed");
                                alerts.push(msg);
                            } else {
                                let msg = format!(
                                    "Spawn failed ({}/{}) for '{}': {}",
                                    count,
                                    3,
                                    truncate_utf8(&item.title, 60),
                                    e
                                );
                                tracing::error!(module = "captain", error = %msg, "spawn failed");
                                alerts.push(msg);
                            }
                        }
                    }
                }
            }
            dispatch_logic::DispatchDecision::NoSlot => break,
            dispatch_logic::DispatchDecision::ResourceBlocked(_)
            | dispatch_logic::DispatchDecision::NotReady => {}
        }
    }
}

/// Clean up after a failed clarifier run: mark session failed, emit
/// timeline event, revert status to New, persist the revert.
///
/// On `Ok(false)` (rev conflict) the revert is dropped silently — the
/// task was concurrently advanced by the HTTP inline reclarifier, which
/// is the authoritative writer for follow-up clarifier work since the
/// `dispatch_reclarify` safety net was removed. The old fallback
/// escalated to captain review on any concurrent-write conflict, which
/// created the PR #966 ghost-failure class: a correctly delivered Q2
/// from the inline path got flipped to captain-reviewing because this
/// tick was still holding a stale in-memory snapshot.
///
/// On `Err(_)` (the query itself failed) we still log but no longer
/// escalate — there is no active writer that needs to be "rescued", and
/// the next tick will re-evaluate the task from fresh DB state.
#[tracing::instrument(skip_all)]
pub(crate) async fn revert_clarifier_start(
    item: &mut Task,
    session_id: &str,
    error: &anyhow::Error,
    pool: &sqlx::SqlitePool,
) {
    if let Err(se) =
        sessions_db::update_session_status(pool, session_id, global_types::SessionStatus::Failed)
            .await
    {
        tracing::warn!(module = "captain", error = %se, "failed to mark clarifier session as failed");
    }

    let err_msg = error.to_string();
    global_infra::best_effort!(
        super::timeline_emit::emit_for_task(
            item,
            &format!("Clarifier failed: {}", truncate_utf8(&err_msg, 120)),
            crate::TimelineEventPayload::ClarifierFailed {
                session_id: session_id.to_string(),
                api_error_status: 0,
                message: err_msg.clone(),
            },
            pool,
        )
        .await,
        "dispatch_redispatch: super::timeline_emit::emit_for_task( item, &format!('Clarifi"
    );
    // Snapshot the in-memory status BEFORE the optimistic transition so
    // we can restore it if the persist fails or loses a rev race. Without
    // restore, the in-memory task would be left at `New` while the DB
    // reflects whatever the concurrent winner wrote (typically
    // `needs-clarification` after the HTTP inline apply). Tick write-back
    // via `update_task_exec` would then call
    // `infer_transition_command(NeedsClarification, New)` which has no
    // legal edge and fails the whole tick merge — codex P1 on PR #966.
    let previous_status = item.status;
    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::New) {
        tracing::error!(
            module = "captain",
            item_id = item.id,
            error = %e,
            "illegal clarifier revert transition"
        );
        return;
    }
    item.last_activity_at = Some(global_types::now_rfc3339());
    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".into(),
        summary: "Retrying clarifier after failure".into(),
        data: TimelineEventPayload::StatusChangedClarifierFail {
            from: api_types::ItemStatus::Clarifying,
            to: api_types::ItemStatus::New,
            session_id: session_id.to_string(),
            error: err_msg.clone(),
        },
    };
    match crate::io::queries::tasks::persist_status_transition_with_command(
        pool,
        item,
        "clarifying",
        "retry_clarifier",
        &event,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!(
                module = "captain",
                id = item.id,
                "clarify revert skipped — task was concurrently advanced (e.g. HTTP inline reclarifier committed a fresh result)"
            );
            // Restore in-memory status so the subsequent tick write-back
            // doesn't fabricate an illegal `NeedsClarification -> New`
            // transition via update_task_exec.
            lifecycle::restore_status(item, previous_status);
        }
        Err(pe) => {
            tracing::error!(
                module = "captain", id = item.id, error = %pe,
                "failed to persist clarify revert"
            );
            lifecycle::restore_status(item, previous_status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_project() -> sqlx::SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        db.pool().clone()
    }

    async fn clarifying_task(pool: &sqlx::SqlitePool, title: &str, session_id: &str) -> Task {
        let wb_id = crate::io::test_support::seed_workbench(pool, 1).await;
        let mut task = Task::new(title);
        task.project_id = 1;
        task.project = "test".into();
        task.workbench_id = wb_id;
        task.status = ItemStatus::Clarifying;
        task.session_ids.clarifier = Some(session_id.into());
        task.last_activity_at = Some(global_types::now_rfc3339());
        task
    }

    /// PR #966 invariant: when `persist_status_transition_with_command`
    /// returns `Ok(false)` (the task was concurrently advanced past
    /// `Clarifying` — e.g. the HTTP inline reclarifier committed a fresh
    /// result between our stream read and our revert), `revert_clarifier_start`
    /// must NOT escalate to captain review. It logs and drops.
    ///
    /// Before PR #966, the `Ok(false)` arm called `reset_review_retry`
    /// which forced the in-memory task into `CaptainReviewing`. At
    /// tick-end persist, that flipped a correctly delivered Q2 into a
    /// ghost captain-review retry (the task-#88 symptom).
    #[tokio::test]
    async fn rev_conflict_does_not_escalate() {
        let pool = pool_with_project().await;

        // Create task as NeedsClarification in the DB — so the
        // `expected_status == "clarifying"` guard in
        // persist_status_transition_with_command will see a mismatch and
        // return Ok(false).
        let mut db_task = clarifying_task(&pool, "rev-conflict", "sid-conflict").await;
        db_task.status = ItemStatus::NeedsClarification;
        let id = crate::io::queries::tasks::insert_task(&pool, &db_task)
            .await
            .unwrap();

        // In-memory task simulates the captain tick snapshot: task was
        // Clarifying when loaded, but the DB has moved on.
        let mut memory_task = crate::io::queries::tasks::find_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();
        memory_task.status = ItemStatus::Clarifying;

        revert_clarifier_start(
            &mut memory_task,
            "sid-conflict",
            &anyhow::anyhow!("simulated CC error"),
            &pool,
        )
        .await;

        // The rev-conflict fallback must:
        //  (a) NOT flip to CaptainReviewing (the pre-PR-#966 behavior
        //      that caused the task-#88 ghost captain-review);
        //  (b) restore the pre-transition in-memory status (Clarifying)
        //      so the tick's write-back doesn't try to infer an illegal
        //      `NeedsClarification -> New` transition — codex P1 fix on
        //      the same PR.
        assert_eq!(
            memory_task.status,
            ItemStatus::Clarifying,
            "in-memory status should be restored to pre-transition value on rev conflict"
        );
        assert!(
            memory_task.captain_review_trigger.is_none(),
            "rev-conflict fallback must not set captain_review_trigger"
        );

        // DB is untouched by the revert (the concurrent winner's
        // state remains).
        let db_after = crate::io::queries::tasks::find_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(db_after.status, ItemStatus::NeedsClarification);
    }
}
