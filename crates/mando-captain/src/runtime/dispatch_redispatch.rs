//! Re-dispatch pass — spawn workers for items that became Queued during the
//! same tick (e.g. after clarification completes).

use std::collections::{HashMap, HashSet};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use crate::biz::dispatch_logic;
use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;

const MAX_SPAWN_FAILS: i64 = 3;

/// Dispatch newly-Queued items that were clarified in this tick.
///
/// `already_dispatched` contains IDs of items that were already Queued at the
/// start of the tick and were dispatched (or attempted) in the first pass.
#[allow(clippy::too_many_arguments)]
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
                            item.status = ItemStatus::InProgress;
                            item.worker = Some(spawn_result.session_name.clone());
                            item.branch = Some(spawn_result.branch);
                            item.worktree = Some(spawn_result.worktree);
                            item.worker_started_at = Some(spawn_result.started_at);
                            item.session_ids.worker = Some(spawn_result.session_id);
                            item.spawn_fail_count = 0;
                            *active_workers += 1;
                            let resource = item
                                .resource
                                .as_deref()
                                .unwrap_or(dispatch_logic::DEFAULT_RESOURCE)
                                .to_string();
                            *resource_counts.entry(resource).or_insert(0) += 1;

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

                            let _ = super::timeline_emit::emit_for_task(
                                item,
                                mando_types::timeline::TimelineEventType::WorkerSpawned,
                                &format!("Spawned {}", spawn_result.session_name),
                                serde_json::json!({"worker": spawn_result.session_name, "session_id": item.session_ids.worker}),
                                pool,
                            )
                            .await;

                            let msg = format!(
                                "\u{1f477} Spawned \u{2192} {}: <b>{}</b>",
                                spawn_result.session_name,
                                mando_shared::telegram_format::escape_html(&item.title),
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
            dispatch_logic::DispatchDecision::NoSlot => break,
            dispatch_logic::DispatchDecision::ResourceBlocked(_)
            | dispatch_logic::DispatchDecision::NotReady => {}
        }
    }
}

/// Clean up after a failed clarifier run: mark session failed, emit
/// timeline event, revert status to New, persist the revert. If the
/// revert persist fails, escalate to captain review.
pub(crate) async fn revert_clarifier_start(
    item: &mut Task,
    session_id: &str,
    error: &anyhow::Error,
    pool: &sqlx::SqlitePool,
) {
    if let Err(se) = mando_db::queries::sessions::update_session_status(
        pool,
        session_id,
        mando_types::SessionStatus::Failed,
    )
    .await
    {
        tracing::warn!(module = "captain", error = %se, "failed to mark clarifier session as failed");
    }

    let err_msg = error.to_string();
    let _ = super::timeline_emit::emit_for_task(
        item,
        TimelineEventType::Errored,
        &format!("Clarifier failed: {}", truncate_utf8(&err_msg, 120)),
        serde_json::json!({"session_id": session_id}),
        pool,
    )
    .await;

    item.status = ItemStatus::New;
    if let Err(pe) = mando_db::queries::tasks::persist_clarify_start(pool, item).await {
        tracing::error!(
            module = "captain", id = item.id, error = %pe,
            "failed to persist clarify revert — escalating to captain review"
        );
        super::action_contract::reset_review_retry(
            item,
            mando_types::task::ReviewTrigger::ClarifierFail,
        );
    }
}
