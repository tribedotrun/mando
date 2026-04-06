//! Parallel clarification dispatch — runs clarifier sessions concurrently.

use std::collections::HashMap;

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::biz::dispatch_logic;
use crate::runtime::clarifier::{self, ClarifierStatus};
use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;

struct ClarifyJob {
    idx: usize,
    session_id: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn clarify_new_items(
    items: &mut [Task],
    config: &Config,
    active_workers: usize,
    max_workers: usize,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    alerts: &mut Vec<String>,
    resource_limits: &HashMap<String, usize>,
    max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
) {
    let new_items = dispatch_logic::new_items(items);
    if new_items.is_empty() {
        return;
    }

    // Skip clarification entirely when no worker slots are available.
    if active_workers >= max_workers {
        return;
    }

    // Cap parallel clarifications at available worker slots to avoid
    // overwhelming the LLM provider with a burst of concurrent sessions.
    let max_parallel = max_workers.saturating_sub(active_workers);

    // Phase 1: Pre-process — set Clarifying status, persist to DB, log sessions.
    let mut jobs: Vec<ClarifyJob> = Vec::new();
    for idx in new_items {
        if jobs.len() >= max_parallel {
            break;
        }
        if dry_run {
            dry_actions.push(format!(
                "would clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
            continue;
        }

        let session_id = mando_uuid::Uuid::v4().to_string();
        let item = &mut items[idx];
        item.status = ItemStatus::Clarifying;
        item.session_ids.clarifier = Some(session_id.clone());

        if let Err(e) = mando_db::queries::tasks::persist_clarify_start(pool, item).await {
            tracing::error!(
                module = "captain",
                id = item.id,
                error = %e,
                "failed to persist clarify start — skipping clarifier this tick"
            );
            item.status = ItemStatus::New;
            item.session_ids.clarifier = None;
            continue;
        }

        let clarifier_cwd = match crate::runtime::clarifier::resolve_clarifier_cwd(item, config) {
            Ok(cwd) => cwd,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "cannot log clarifier session start, skipping this dispatch"
                );
                item.session_ids.clarifier = None;
                continue;
            }
        };
        if let Err(e) = crate::io::headless_cc::log_cc_session(
            pool,
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &session_id,
                cwd: &clarifier_cwd,
                model: &workflow.models.clarifier,
                caller: "clarifier",
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: &item.id.to_string(),
                status: mando_types::SessionStatus::Running,
                worker_name: "",
            },
        )
        .await
        {
            tracing::warn!(module = "captain", id = item.id, error = %e, "failed to log clarifier session start");
        }

        let _ = super::timeline_emit::emit_for_task(
            item,
            mando_types::timeline::TimelineEventType::ClarifyStarted,
            "Clarification starting",
            serde_json::json!({"session_id": &session_id}),
            pool,
        )
        .await;

        jobs.push(ClarifyJob { idx, session_id });
    }

    if jobs.is_empty() {
        return;
    }

    // Phase 2: Run all clarifications in parallel.
    let task_snapshots: Vec<Task> = jobs.iter().map(|j| items[j.idx].clone()).collect();
    let futs: Vec<_> = jobs
        .iter()
        .zip(task_snapshots.iter())
        .map(|(job, task)| {
            clarifier::run_clarification(task, workflow, config, pool, Some(&job.session_id))
        })
        .collect();
    let results = futures::future::join_all(futs).await;

    // Phase 3: Apply results sequentially.
    for (job, result) in jobs.iter().zip(results) {
        match result {
            Ok(r) => {
                apply_clarifier_ok(
                    &mut items[job.idx],
                    r,
                    config,
                    notifier,
                    resource_limits,
                    pool,
                )
                .await;
            }
            Err(e) => {
                apply_clarifier_err(
                    &mut items[job.idx],
                    &job.session_id,
                    e,
                    notifier,
                    alerts,
                    max_clarifier_retries,
                    pool,
                )
                .await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_clarifier_ok(
    item: &mut Task,
    result: clarifier::ClarifierResult,
    config: &Config,
    notifier: &Notifier,
    resource_limits: &HashMap<String, usize>,
    pool: &sqlx::SqlitePool,
) {
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
                    title = %truncate_utf8(&item.title, 60),
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
                    match mando_config::resolve_project_config(Some(&repo), config) {
                        Some((_, pc)) => {
                            item.project = Some(pc.name.clone());
                        }
                        None => {
                            tracing::error!(
                                module = "captain",
                                repo = %repo,
                                title = %truncate_utf8(&item.title, 60),
                                "clarifier returned repo that passed schema but fails resolve_project_config; config/schema mismatch; escalating"
                            );
                            super::action_contract::reset_review_retry(
                                item,
                                mando_types::task::ReviewTrigger::ClarifierFail,
                            );
                            return;
                        }
                    }
                }
                if let Some(no_pr) = result.no_pr {
                    item.no_pr = no_pr;
                }
                if let Some(ref resource) = result.resource {
                    let is_known = resource == "cc" || resource_limits.contains_key(resource);
                    if is_known {
                        item.resource = Some(resource.clone());
                    } else {
                        tracing::warn!(
                            module = "captain",
                            resource = %resource,
                            title = %truncate_utf8(&item.title, 60),
                            "clarifier returned unknown resource — ignoring"
                        );
                    }
                }

                if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                    tracing::error!(
                        module = "captain",
                        id = item.id,
                        error = %e,
                        "failed to persist clarify result"
                    );
                }

                let _ = super::timeline_emit::emit_for_task(
                    item,
                    mando_types::timeline::TimelineEventType::ClarifyResolved,
                    "Clarification complete, ready for work",
                    serde_json::json!({"session_id": result.session_id}),
                    pool,
                )
                .await;

                tracing::info!(
                    module = "captain",
                    title = %truncate_utf8(&item.title, 60),
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

            if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "failed to persist clarify result"
                );
            }

            let _ = super::timeline_emit::emit_for_task(
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

            if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "failed to persist clarify result"
                );
            }

            let _ = super::timeline_emit::emit_for_task(
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

async fn apply_clarifier_err(
    item: &mut Task,
    session_id: &str,
    e: anyhow::Error,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
) {
    super::dispatch_redispatch::revert_clarifier_start(item, session_id, &e, pool).await;
    // Rate-limit-caused failure — activate cooldown, skip retry count.
    if super::rate_limit_cooldown::check_and_activate_from_stream(session_id) {
        let _ = super::timeline_emit::emit_rate_limited(item, pool).await;
        return;
    }
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
            truncate_utf8(&item.title, 60),
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
