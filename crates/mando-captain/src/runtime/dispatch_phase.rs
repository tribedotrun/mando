//! Dispatch phase — dispatch ready/new items to workers.

use std::collections::{HashMap, HashSet};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::biz::dispatch_logic;
use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;

/// Dispatch ready and new items to workers.
///
/// Returns the updated active worker count.
#[allow(clippy::too_many_arguments)]
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
) -> usize {
    let mut resource_counts = dispatch_logic::count_resources(items);
    let max_clarifier_retries = workflow.agent.max_clarifier_retries as i64;
    const MAX_SPAWN_FAILS: i64 = 3;

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
                            item.status = ItemStatus::InProgress;
                            item.worker = Some(spawn_result.session_name.clone());
                            item.branch = Some(spawn_result.branch);
                            item.worktree = Some(spawn_result.worktree);
                            item.worker_started_at = Some(spawn_result.started_at);
                            item.session_ids.worker = Some(spawn_result.session_id);
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
                                mando_db::queries::tasks::persist_spawn(pool, item).await
                            {
                                tracing::error!(module = "captain", id = item.id, error = %e,
                                    "failed to persist spawn — killing orphan worker");
                                if let Some(ref cc_sid) = item.session_ids.worker {
                                    crate::io::session_terminate::terminate_session(
                                        pool,
                                        cc_sid,
                                        mando_types::SessionStatus::Failed,
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
                            let _ = super::timeline_emit::emit_for_task(
                                item,
                                mando_types::timeline::TimelineEventType::WorkerSpawned,
                                &format!("Spawned {}", spawn_result.session_name),
                                serde_json::json!({"worker": spawn_result.session_name, "session_id": item.session_ids.worker}),
                                pool,
                            )
                            .await;

                            let msg = format!(
                                "\u{1f477} Spawned → {}: <b>{}</b>",
                                spawn_result.session_name,
                                mando_shared::telegram_format::escape_html(&item.title),
                            );
                            notifier.normal(&msg).await;
                        }
                        Err(e) => {
                            let item = &mut items[idx];
                            item.worker_seq -= 1; // Roll back — no worker was spawned.
                            let count = item.spawn_fail_count + 1;
                            item.spawn_fail_count = count;

                            if count >= MAX_SPAWN_FAILS {
                                super::action_contract::reset_review_retry(
                                    item,
                                    mando_types::task::ReviewTrigger::ClarifierFail,
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
    )
    .await;

    super::dispatch_reclarify::reclarify_items(
        items,
        config,
        workflow,
        dry_run,
        dry_actions,
        alerts,
        max_clarifier_retries,
        pool,
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
#[cfg(test)]
#[path = "dispatch_phase_tests.rs"]
mod tests;
