//! Dispatch phase — dispatch ready/new items to workers.

use std::collections::{HashMap, HashSet};

use crate::{ItemStatus, Task};
use global_bus::EventBus;
use settings::CaptainWorkflow;
use settings::Config;
use tokio_util::task::TaskTracker;

use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::dispatch_redispatch::MAX_SPAWN_FAILS;
use crate::runtime::notify::Notifier;
use crate::service::{dispatch_logic, lifecycle};

/// Dispatch ready and new items to workers.
///
/// Returns the updated active worker count.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(crate) async fn dispatch_new_work(
    items: &mut [Task],
    config: &Config,
    mut active_workers: usize,
    max_workers: usize,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    alerts: &mut Vec<String>,
    resource_limits: &HashMap<String, usize>,
    pool: &sqlx::SqlitePool,
    bus: Option<&EventBus>,
    task_tracker: &TaskTracker,
) -> usize {
    // Dispatch planning-mode items first (they don't consume worker slots).
    super::dispatch_planning::dispatch_planning_items(
        items,
        config,
        workflow,
        pool,
        bus,
        dry_run,
        dry_actions,
        task_tracker,
    )
    .await;

    let mut resource_counts = dispatch_logic::count_resources(items);
    let max_clarifier_retries = workflow.agent.max_clarifier_retries as i64;
    let mut needs_live_refresh = false;

    // Dispatch ready/rework items. Track IDs so the redispatch pass skips them.
    let dispatchable = dispatch_logic::dispatchable_items(items);
    let already_dispatched: HashSet<i64> = dispatchable.iter().map(|&i| items[i].id).collect();
    for idx in dispatchable {
        let item = &items[idx];
        let decision = dispatch_logic::check_dispatch(
            item,
            active_workers,
            max_workers,
            resource_limits,
            &resource_counts,
        );

        match decision {
            dispatch_logic::DispatchDecision::Spawn => {
                if dry_run {
                    dry_actions.push(format!(
                        "would spawn worker for '{}'",
                        truncate_utf8(&item.title, 60)
                    ));
                    active_workers += 1;
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
                                    "illegal dispatch transition"
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
                            item.pr_number = spawn_result.pr_number;
                            item.spawn_fail_count = 0;
                            active_workers += 1;
                            let resource = item
                                .resource
                                .as_deref()
                                .unwrap_or(dispatch_logic::DEFAULT_RESOURCE)
                                .to_string();
                            *resource_counts.entry(resource).or_insert(0) += 1;

                            // Persist worker fields immediately so the DB
                            // reflects the running worker even if captain
                            // crashes before tick-end merge.
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
                                active_workers -= 1;
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

                            // Emit timeline event with session_id.
                            global_infra::best_effort!(super::timeline_emit::emit_for_task(
                                item,
                                &format!("Spawned {}", spawn_result.session_name),
                                crate::TimelineEventPayload::WorkerSpawned {
                                    worker: spawn_result.session_name.clone(),
                                    session_id: item.session_ids.worker.clone().unwrap_or_default(),
                                },
                                pool,
                            )
                            .await, "dispatch_phase: super::timeline_emit::emit_for_task( item, &format!('Spawned");
                            let msg = format!(
                                "\u{1f477} Spawned → {}: <b>{}</b>",
                                spawn_result.session_name,
                                global_infra::html::escape_html(&item.title),
                            );
                            notifier.normal(&msg).await;
                            needs_live_refresh = true;
                        }
                        Err(e) => {
                            let item = &mut items[idx];
                            item.worker_seq -= 1; // Roll back — no worker was spawned.
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
            dispatch_logic::DispatchDecision::NoSlot => {
                tracing::debug!(module = "captain", title = %item.title, "no slot available");
                break;
            }
            dispatch_logic::DispatchDecision::ResourceBlocked(res) => {
                tracing::debug!(module = "captain", resource = %res, title = %item.title, "resource at limit");
            }
            dispatch_logic::DispatchDecision::NotReady => {}
        }
    }
    if needs_live_refresh {
        let dispatched_ids: Vec<i64> = already_dispatched.iter().copied().collect();
        emit_live_refresh(bus, &dispatched_ids);
    }
    // Dispatch new items to clarifier (parallel).
    super::dispatch_clarify::clarify_new_items(
        items,
        config,
        active_workers,
        max_workers,
        workflow,
        notifier,
        dry_run,
        dry_actions,
        alerts,
        resource_limits,
        max_clarifier_retries,
        pool,
        bus,
        task_tracker,
    )
    .await;

    // Re-dispatch: items that became Queued during clarification can be
    // dispatched in the same tick instead of waiting 30s for the next one.
    super::dispatch_redispatch::redispatch_newly_queued(
        items,
        config,
        &mut active_workers,
        max_workers,
        workflow,
        notifier,
        dry_run,
        dry_actions,
        alerts,
        resource_limits,
        &mut resource_counts,
        pool,
        &already_dispatched,
    )
    .await;

    active_workers
}

fn emit_live_refresh(bus: Option<&EventBus>, affected_task_ids: &[i64]) {
    if let Some(bus) = bus {
        bus.send(global_bus::BusPayload::Tasks(None));
        bus.send(global_bus::BusPayload::Sessions(Some(
            api_types::SessionsEventData {
                affected_task_ids: Some(affected_task_ids.to_vec()),
            },
        )));
    }
}

#[cfg(test)]
#[path = "dispatch_phase_tests.rs"]
mod tests;
