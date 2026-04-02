//! Dispatch phase — dispatch ready/new items to workers.

use std::collections::HashMap;

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::biz::dispatch_logic;
use crate::runtime::clarifier::{self, ClarifierStatus};
use crate::runtime::linear_integration;
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
) -> Result<usize> {
    let mut resource_counts = dispatch_logic::count_resources(items);
    let max_clarifier_retries = workflow.agent.max_clarifier_retries as i64;
    const MAX_SPAWN_FAILS: i64 = 3;

    // Dispatch ready/rework items.
    let dispatchable = dispatch_logic::dispatchable_items(items);
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
                        &item.title[..item.title.len().min(60)]
                    ));
                    active_workers += 1;
                    let resource = item.resource.as_deref().unwrap_or("cc").to_string();
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
                            let resource = item.resource.as_deref().unwrap_or("cc").to_string();
                            *resource_counts.entry(resource).or_insert(0) += 1;

                            // Persist worker fields immediately so the DB
                            // reflects the running worker even if captain
                            // crashes before tick-end merge.
                            if let Err(e) =
                                mando_db::queries::tasks::persist_spawn(pool, item).await
                            {
                                tracing::error!(
                                    module = "captain",
                                    id = item.id,
                                    error = %e,
                                    "failed to persist spawn — orphan risk"
                                );
                            }

                            // Emit timeline event with session_id.
                            super::timeline_emit::emit_for_task(
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

                            // Linear writeback: InProgress.
                            if let Err(e) = linear_integration::writeback_status(item, config).await
                            {
                                tracing::warn!(module = "captain", %e, "Linear status writeback failed");
                            }
                            if let Err(e) = linear_integration::upsert_workpad(
                                item,
                                config,
                                &format!("Worker spawned, working on: {}", item.title),
                                pool,
                            )
                            .await
                            {
                                tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
                            }
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
                                    &item.title[..item.title.len().min(60)],
                                    e
                                );
                                tracing::error!(module = "captain", error = %msg, "spawn permanently failed");
                                alerts.push(msg);
                            } else {
                                let msg = format!(
                                    "Spawn failed ({}/{}) for '{}': {}",
                                    count,
                                    3,
                                    &item.title[..item.title.len().min(60)],
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

    // Dispatch new items to clarifier.
    let new_items = dispatch_logic::new_items(items);
    for idx in new_items {
        if active_workers >= max_workers {
            break;
        }
        if dry_run {
            dry_actions.push(format!(
                "would clarify '{}'",
                &items[idx].title[..items[idx].title.len().min(60)]
            ));
            continue;
        }

        let linear_cli = &config.captain.linear_cli_path;
        match clarifier::run_clarification(&items[idx], linear_cli, workflow, config, pool).await {
            Ok(result) => {
                let item = &mut items[idx];
                match result.status {
                    ClarifierStatus::Ready => {
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }
                        // Quality gate: validate clarifier output is substantive (≥20 chars).
                        let context_trimmed: String = result
                            .context
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect();
                        if context_trimmed.len() < 20 {
                            tracing::warn!(
                                module = "captain",
                                title = %&item.title[..item.title.len().min(60)],
                                context_len = context_trimmed.len(),
                                "clarifier returned trivial context (<20 chars), escalating to captain review"
                            );
                            super::action_contract::reset_review_retry(
                                item,
                                mando_types::task::ReviewTrigger::ClarifierFail,
                            );
                            item.context = Some(result.context);
                            let msg = format!(
                                "\u{26a0}\u{fe0f} Clarifier returned trivial output for <b>{}</b> — needs human input",
                                mando_shared::telegram_format::escape_html(&item.title),
                            );
                            notifier.high(&msg).await;
                        } else {
                            item.status = ItemStatus::Queued;
                            item.clarifier_fail_count = 0;
                            item.context = Some(result.context);
                            if let Some(title) = result.generated_title {
                                if !title.is_empty() {
                                    item.title = title;
                                }
                            }
                            if let Some(repo) = result.repo.filter(|r| !r.trim().is_empty()) {
                                let (_, pc) =
                                    mando_config::resolve_project_config(Some(&repo), config)
                                        .unwrap_or_else(|| {
                                            panic!(
                                                "clarifier returned repo '{}' which passed schema \
                                                 validation but fails resolve_project_config — \
                                                 config/schema mismatch",
                                                repo
                                            )
                                        });
                                item.project = Some(pc.name.clone());
                            }
                            if let Some(no_pr) = result.no_pr {
                                item.no_pr = no_pr;
                            }
                            if let Some(ref resource) = result.resource {
                                let is_known =
                                    resource == "cc" || resource_limits.contains_key(resource);
                                if is_known {
                                    item.resource = Some(resource.clone());
                                } else {
                                    tracing::warn!(
                                        module = "captain",
                                        resource = %resource,
                                        title = %&item.title[..item.title.len().min(60)],
                                        "clarifier returned unknown resource — ignoring"
                                    );
                                }
                            }

                            // Emit clarify-resolved timeline event.
                            super::timeline_emit::emit_for_task(
                                item,
                                mando_types::timeline::TimelineEventType::ClarifyResolved,
                                "Clarification complete, ready for work",
                                serde_json::json!({"session_id": result.session_id}),
                                pool,
                            )
                            .await;

                            tracing::info!(
                                module = "captain",
                                title = %&item.title[..item.title.len().min(60)],
                                "clarified, now ready"
                            );
                        }
                    }
                    ClarifierStatus::Clarifying => {
                        item.status = ItemStatus::NeedsClarification;
                        item.last_activity_at = Some(mando_types::now_rfc3339());
                        item.context = Some(result.context);
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }

                        // Emit clarify-question timeline event with structured questions.
                        super::timeline_emit::emit_for_task(
                            item,
                            mando_types::timeline::TimelineEventType::ClarifyQuestion,
                            "Needs clarification",
                            serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                            pool,
                        )
                        .await;

                        if let Some(ref questions) = result.questions {
                            let text = clarifier::format_questions_text(questions);
                            let msg = format!(
                                "\u{2753} Needs clarification: <b>{}</b>\n{}",
                                mando_shared::telegram_format::escape_html(&item.title),
                                mando_shared::telegram_format::escape_html(&text),
                            );
                            notifier
                                .notify_typed(
                                    &msg,
                                    mando_types::notify::NotifyLevel::High,
                                    mando_types::events::NotificationKind::NeedsClarification {
                                        item_id: item.id.to_string(),
                                        questions: Some(text),
                                    },
                                    Some(&item.id.to_string()),
                                )
                                .await;
                        }
                    }
                    ClarifierStatus::Escalate => {
                        super::action_contract::reset_review_retry(
                            item,
                            mando_types::task::ReviewTrigger::ClarifierFail,
                        );
                        item.context = Some(result.context);
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }

                        // Emit clarify-question timeline event.
                        super::timeline_emit::emit_for_task(
                            item,
                            mando_types::timeline::TimelineEventType::ClarifyQuestion,
                            "Needs human input",
                            serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                            pool,
                        )
                        .await;

                        if let Some(ref questions) = result.questions {
                            let text = clarifier::format_questions_text(questions);
                            let msg = format!(
                                "\u{2753} Needs human input: <b>{}</b>\n{}",
                                mando_shared::telegram_format::escape_html(&item.title),
                                mando_shared::telegram_format::escape_html(&text),
                            );
                            notifier
                                .notify_typed(
                                    &msg,
                                    mando_types::notify::NotifyLevel::High,
                                    mando_types::events::NotificationKind::Escalated {
                                        item_id: item.id.to_string(),
                                        summary: Some(text),
                                    },
                                    Some(&item.id.to_string()),
                                )
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                let item = &mut items[idx];
                let count = item.clarifier_fail_count + 1;
                item.clarifier_fail_count = count;

                if count >= max_clarifier_retries {
                    super::action_contract::reset_review_retry(
                        item,
                        mando_types::task::ReviewTrigger::ClarifierFail,
                    );
                    let msg = format!(
                        "Clarifier failed {} times for '{}', escalated to captain review: {}",
                        count,
                        &item.title[..item.title.len().min(60)],
                        e
                    );
                    tracing::error!(module = "captain", error = %msg, "clarifier permanently failed");
                    alerts.push(msg.clone());
                    notifier
                        .high(&format!(
                            "\u{274c} Clarifier failed {} times for <b>{}</b> — needs human",
                            count,
                            mando_shared::telegram_format::escape_html(&item.title),
                        ))
                        .await;
                } else {
                    tracing::warn!(
                        module = "captain",
                        title = %item.title,
                        attempt = count,
                        error = %e,
                        "clarifier failed, will retry on next tick"
                    );
                }
            }
        }
    }

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

    Ok(active_workers)
}

#[cfg(test)]
#[path = "dispatch_phase_tests.rs"]
mod tests;
