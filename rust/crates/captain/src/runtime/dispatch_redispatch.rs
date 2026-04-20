//! Re-dispatch pass — spawn workers for items that became Queued during the
//! same tick (e.g. after clarification completes).

use std::collections::{HashMap, HashSet};

use crate::{ItemStatus, Task, TimelineEventPayload};
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

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
                            item.workbench_id = spawn_result.workbench_id;
                            item.worker_started_at = Some(spawn_result.started_at);
                            item.session_ids.worker = Some(spawn_result.session_id);
                            item.plan = spawn_result.plan;
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
/// timeline event, revert status to New, persist the revert. If the
/// revert persist fails, escalate to captain review.
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
            from: "clarifying".to_string(),
            to: "new".to_string(),
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
            tracing::error!(
                module = "captain",
                id = item.id,
                "failed to persist clarify revert — task changed concurrently"
            );
            super::action_contract::reset_review_retry(item, crate::ReviewTrigger::ClarifierFail);
        }
        Err(pe) => {
            tracing::error!(
                module = "captain", id = item.id, error = %pe,
                "failed to persist clarify revert — escalating to captain review"
            );
            super::action_contract::reset_review_retry(item, crate::ReviewTrigger::ClarifierFail);
        }
    }
}
