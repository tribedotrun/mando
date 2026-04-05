//! Re-clarification loop — safety net for items stuck in Clarifying status.
//!
//! Runs all re-clarification CC sessions in parallel via `futures::join_all`.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::biz::dispatch_logic;
use crate::runtime::clarifier::{self, ClarifierStatus};
use crate::runtime::dashboard::truncate_utf8;

/// Re-clarify items where human answered (Clarifying status).
/// Safety net for items that got stuck or where the inline call failed.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn reclarify_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    alerts: &mut Vec<String>,
    max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
) {
    // Only re-clarify items in Clarifying status (human answered but inline
    // call failed). NeedsClarification items are waiting for human input —
    // don't re-run the clarifier and produce events that strip self-answered context.
    let clarifying: Vec<usize> = dispatch_logic::clarifying_items(items)
        .into_iter()
        .filter(|&idx| items[idx].status == ItemStatus::Clarifying)
        .collect();

    let mut jobs: Vec<usize> = Vec::new();
    for idx in clarifying {
        if dry_run {
            dry_actions.push(format!(
                "would re-clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
        } else {
            jobs.push(idx);
        }
    }

    if jobs.is_empty() {
        return;
    }

    // Run all re-clarifications in parallel.
    let hint = "(review the context above and decide if the item is ready)";
    let task_snapshots: Vec<Task> = jobs.iter().map(|&idx| items[idx].clone()).collect();
    let futs: Vec<_> = task_snapshots
        .iter()
        .map(|task| clarifier::answer_and_reclarify(task, hint, workflow, config, pool))
        .collect();
    let results = futures::future::join_all(futs).await;

    // Apply results sequentially.
    for (&idx, result) in jobs.iter().zip(results) {
        match result {
            Ok(result) => {
                apply_reclarify_ok(&mut items[idx], result, pool).await;
            }
            Err(e) => {
                apply_reclarify_err(&mut items[idx], e, max_clarifier_retries, alerts, pool).await;
            }
        }
    }
}

async fn apply_reclarify_ok(
    item: &mut Task,
    result: clarifier::ClarifierResult,
    pool: &sqlx::SqlitePool,
) {
    item.clarifier_fail_count = 0;
    match result.status {
        ClarifierStatus::Ready => {
            item.status = ItemStatus::Queued;
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }

            super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::ClarifyResolved,
                "Re-clarification complete, ready for work",
                serde_json::json!({"session_id": result.session_id}),
                pool,
            )
            .await;

            tracing::info!(
                module = "captain",
                title = %truncate_utf8(&item.title, 60),
                "re-clarified, now ready"
            );
        }
        ClarifierStatus::Clarifying => {
            // Only stamp last_activity_at on the initial transition
            // so the timeout clock doesn't reset every tick.
            if item.status != ItemStatus::NeedsClarification {
                item.last_activity_at = Some(mando_types::now_rfc3339());
            }
            item.status = ItemStatus::NeedsClarification;
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }

            super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::ClarifyQuestion,
                "Still needs clarification",
                serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                pool,
            )
            .await;
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
        }
    }
}

async fn apply_reclarify_err(
    item: &mut Task,
    e: anyhow::Error,
    max_clarifier_retries: i64,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) {
    // Rate-limit-caused failure — activate cooldown, skip retry count.
    if let Some(ref sid) = item.session_ids.clarifier {
        if super::rate_limit_cooldown::check_and_activate_from_stream(sid) {
            super::timeline_emit::emit_rate_limited(item, pool).await;
            return;
        }
    }
    let count = item.clarifier_fail_count + 1;
    item.clarifier_fail_count = count;

    if count >= max_clarifier_retries {
        super::action_contract::reset_review_retry(
            item,
            mando_types::task::ReviewTrigger::ClarifierFail,
        );
        tracing::error!(
            module = "captain",
            title = %item.title,
            attempt = count,
            error = %e,
            "re-clarification failed 3 times — escalating"
        );
        alerts.push(format!(
            "Re-clarification failed {} times for '{}': {}",
            count,
            truncate_utf8(&item.title, 60),
            e
        ));
    } else {
        tracing::warn!(
            module = "captain",
            title = %item.title,
            attempt = count,
            error = %e,
            "re-clarification failed — will retry on next tick"
        );
    }
}
