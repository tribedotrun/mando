use anyhow::{bail, Result};
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use crate::runtime::task_notes::append_tagged_note;
use crate::service::{lifecycle, spawn_logic};
use crate::{ItemStatus, ReviewTrigger, Task};

use super::{captain_review, timeline_emit, Notifier};

pub enum ReopenOutcome {
    Reopened,
    QueuedFallback,
    CaptainReviewing,
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn reopen_item(
    item: &mut Task,
    reopen_source: &str,
    feedback: &str,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    allow_queue_fallback: bool,
) -> Result<ReopenOutcome> {
    if reopen_source == "human"
        && (item.status == ItemStatus::CaptainReviewing
            || item.status == ItemStatus::CaptainMerging)
    {
        anyhow::bail!(
            "cannot reopen item {}: captain {} is in progress",
            item.id,
            if item.status == ItemStatus::CaptainReviewing {
                "review"
            } else {
                "merge"
            }
        );
    }

    let item_id = item.id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&item_id, "reopen")?;
    if let Some(new_context) =
        append_tagged_note(item.context.as_deref(), "Reopen feedback", feedback)
    {
        item.context = Some(new_context);
    }

    if reopen_source == "human" {
        item.intervention_count = 0;
    }

    let budget = spawn_logic::check_intervention(
        item.intervention_count as u32,
        1,
        workflow.agent.max_interventions,
    );
    let new_count = match budget {
        spawn_logic::InterventionResult::Proceed { new_count } => new_count,
        spawn_logic::InterventionResult::Exhausted { new_count } => {
            item.intervention_count = new_count as i64;
            item.last_activity_at = Some(global_types::now_rfc3339());
            trigger_review(
                item,
                ReviewTrigger::BudgetExhausted,
                config,
                workflow,
                notifier,
                pool,
            )
            .await?;
            return Ok(ReopenOutcome::CaptainReviewing);
        }
    };

    item.reopen_source = Some(reopen_source.to_string());
    let can_resume =
        item.worker.is_some() && item.session_ids.worker.is_some() && item.worktree.is_some();
    if !can_resume {
        if allow_queue_fallback {
            item.intervention_count = new_count as i64;
            item.reopen_seq += 1;
            item.reopened_at = Some(global_types::now_rfc3339());
            // apply_transition returns a TaskTransitionDecision describing
            // the resolved state; callers that don't consume it pay no
            // business-logic penalty, but clippy's must_use check surfaces
            // the discard. Bind to `_decision` so the value is still
            // observable in a debugger while the lint is satisfied.
            let _decision = lifecycle::apply_transition(item, ItemStatus::Queued)?;
            item.pr_number = None;
            item.worker = None;
            item.worktree = None;
            item.branch = None;
            item.worker_started_at = None;
            item.session_ids.worker = None;
            item.session_ids.ask = None;
            item.last_activity_at = Some(global_types::now_rfc3339());
            try_unarchive_workbench(item, "during queued fallback", pool).await;
            emit_reopen_event(item, reopen_source, feedback, "queued", pool).await;
            return Ok(ReopenOutcome::QueuedFallback);
        }
        bail!("item missing worker/session/worktree — cannot reopen");
    }

    let old_worktree = item.worktree.clone();
    try_unarchive_workbench(item, "before reopen", pool).await;
    match super::spawner_lifecycle::reopen_worker(item, config, feedback, workflow, pool).await {
        Ok(result) => {
            item.intervention_count = new_count as i64;
            item.reopen_seq += 1;
            item.reopened_at = Some(global_types::now_rfc3339());
            let _decision = lifecycle::apply_transition(item, ItemStatus::InProgress)?;
            item.worker = Some(result.session_name);
            item.session_ids.worker = Some(result.session_id);
            item.session_ids.ask = None;
            item.branch = Some(result.branch);
            item.worktree = Some(result.worktree.clone());
            let worktree_changed = old_worktree.as_deref() != Some(&result.worktree);
            if worktree_changed {
                let old_wb_id = item.workbench_id;
                item.workbench_id =
                    crate::io::queries::workbenches::find_by_worktree(pool, &result.worktree)
                        .await
                        .ok()
                        .flatten()
                        .map(|wb| wb.id)
                        .unwrap_or(0);
                if old_wb_id != 0 && old_wb_id != item.workbench_id {
                    if let Err(e) = crate::io::queries::workbenches::archive(pool, old_wb_id).await
                    {
                        tracing::warn!(module = "captain-runtime-action_contract-reopen", workbench_id = old_wb_id, error = %e, "failed to archive previous workbench during reopen");
                    }
                }
            }
            item.worker_started_at = Some(global_types::now_rfc3339());
            item.last_activity_at = item.worker_started_at.clone();

            emit_reopen_event(item, reopen_source, feedback, "reopened", pool).await;
            Ok(ReopenOutcome::Reopened)
        }
        Err(e) => {
            tracing::warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "reopen_worker failed — falling back to queue"
            );
            if allow_queue_fallback {
                item.intervention_count = new_count as i64;
                item.reopen_seq += 1;
                // apply_transition returns a TaskTransitionDecision describing
                // the resolved state; callers that don't consume it pay no
                // business-logic penalty, but clippy's must_use check surfaces
                // the discard. Bind to `_decision` so the value is still
                // observable in a debugger while the lint is satisfied.
                let _decision = lifecycle::apply_transition(item, ItemStatus::Queued)?;
                item.reopened_at = Some(global_types::now_rfc3339());
                item.last_activity_at = item.reopened_at.clone();
                emit_reopen_event(item, reopen_source, feedback, "queued", pool).await;
                item.pr_number = None;
                item.worker = None;
                item.worktree = None;
                item.branch = None;
                item.worker_started_at = None;
                item.session_ids.worker = None;
                item.session_ids.ask = None;
                try_unarchive_workbench(item, "during reopen fallback", pool).await;
                Ok(ReopenOutcome::QueuedFallback)
            } else {
                Err(e)
            }
        }
    }
}

#[tracing::instrument(skip_all)]
pub(super) async fn trigger_review(
    item: &mut Task,
    trigger: ReviewTrigger,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let db_status = item.status.as_str().to_string();
    super::reset_review_retry(item, trigger);
    captain_review::spawn_review(
        item,
        trigger.as_str(),
        Some(&db_status),
        config,
        workflow,
        notifier,
        pool,
    )
    .await
}

async fn try_unarchive_workbench(item: &Task, context: &str, pool: &sqlx::SqlitePool) {
    if item.workbench_id == 0 {
        return;
    }
    if let Err(e) = crate::io::queries::workbenches::unarchive(pool, item.workbench_id).await {
        tracing::warn!(module = "captain-runtime-action_contract-reopen", workbench_id = item.workbench_id, error = %e, "failed to unarchive workbench {context}");
    }
}

async fn emit_reopen_event(
    item: &Task,
    source: &str,
    feedback: &str,
    outcome: &str,
    pool: &sqlx::SqlitePool,
) {
    if source == "human" {
        return;
    }
    let source_label = match source {
        "review" => "review comments",
        "ci" => "CI failure",
        "evidence" => "missing evidence",
        _ => source,
    };
    let summary = format!(
        "Auto-reopened for {} (seq {})",
        source_label, item.reopen_seq
    );
    global_infra::best_effort!(
        timeline_emit::emit_for_task(
            item,
            &summary,
            crate::TimelineEventPayload::WorkerReopened {
                source: source.to_string(),
                reopen_seq: item.reopen_seq,
                outcome: outcome.to_string(),
                feedback: feedback.to_string(),
                worker: item.worker.clone().unwrap_or_default(),
                session_id: item.session_ids.worker.clone().unwrap_or_default(),
            },
            pool,
        )
        .await,
        "reopen: timeline_emit::emit_for_task( item, &summary, crate::Timelin"
    );
}
