use anyhow::{bail, Result};
use settings::CaptainWorkflow;
use settings::Config;

use crate::runtime::clear_task_interaction_sessions;
use crate::runtime::spawner_lifecycle::LifecycleResult;
use crate::runtime::task_notes::append_tagged_note;
use crate::service::{lifecycle, spawn_logic};
use crate::{ItemStatus, ReviewTrigger, Task};

use super::{captain_review, timeline_emit, Notifier};

pub enum ReopenOutcome {
    Reopened,
    QueuedFallback,
    CaptainReviewing,
}

/// Project a `LifecycleResult` onto `item`. `pr_number` is absolute-state:
/// resume preserves, fresh-spawn replaces (possibly with `None` on create
/// failure).
fn apply_lifecycle_result(item: &mut Task, result: LifecycleResult) {
    item.worker = Some(result.session_name);
    item.session_ids.worker = Some(result.session_id);
    item.branch = Some(result.branch);
    item.worktree = Some(result.worktree);
    item.pr_number = result.pr_number;
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
            // worktree and workbench_id are permanent once assigned — captain
            // invariant #4 in CLAUDE.md. Next spawn hits the spawner's Rework
            // arm (same worktree, new branch from origin/main).
            item.branch = None;
            item.worker_started_at = None;
            item.session_ids.worker = None;
            clear_task_interaction_sessions(item);
            item.last_activity_at = Some(global_types::now_rfc3339());
            try_unarchive_workbench(item, "during queued fallback", pool).await;
            emit_reopen_event(item, reopen_source, feedback, "queued", pool).await;
            return Ok(ReopenOutcome::QueuedFallback);
        }
        bail!("item missing worker/session/worktree — cannot reopen");
    }

    try_unarchive_workbench(item, "before reopen", pool).await;
    match super::spawner_lifecycle::reopen_worker(item, config, feedback, workflow, pool).await {
        Ok(result) => {
            item.intervention_count = new_count as i64;
            item.reopen_seq += 1;
            item.reopened_at = Some(global_types::now_rfc3339());
            let _decision = lifecycle::apply_transition(item, ItemStatus::InProgress)?;
            // reopen_worker always returns the item's existing worktree —
            // either via resume (same path) or clean_and_spawn_fresh (Rework
            // arm, in-place reset). No workbench swap needed.
            apply_lifecycle_result(item, result);
            clear_task_interaction_sessions(item);
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
                // worktree and workbench_id are permanent — see queued-fallback
                // arm above for the full rationale.
                item.branch = None;
                item.worker_started_at = None;
                item.session_ids.worker = None;
                clear_task_interaction_sessions(item);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queued_fallback_preserves_worktree_when_session_is_missing() {
        // Captain invariant #4 — same task, same worktree. A reopen that
        // falls back to Queued because the worker/session refs are gone
        // (e.g. respawn cleared them) must NOT nuke the task's worktree.
        // Next spawn must reuse it via the spawner's Rework arm.
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        let notifier =
            crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
        let config = settings::Config::default();
        let workflow = settings::CaptainWorkflow::compiled_default();

        let mut item = Task::new("reopen-queued-fallback");
        item.set_status_for_tests(ItemStatus::InProgress);
        // Respawn-cleared state: worktree retained, worker/session/branch gone.
        item.worktree = Some("/tmp/mando-todo-7".into());
        item.workbench_id = 3;
        item.worker = None;
        item.session_ids.worker = None;

        let outcome = reopen_item(
            &mut item,
            "review",
            "please revisit",
            &config,
            &workflow,
            &notifier,
            &pool,
            true,
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ReopenOutcome::QueuedFallback));
        assert_eq!(
            item.worktree.as_deref(),
            Some("/tmp/mando-todo-7"),
            "worktree must survive the queued-fallback branch",
        );
        assert_eq!(item.workbench_id, 3, "workbench_id is permanent");
        assert!(item.branch.is_none(), "branch is cleared");
        assert!(item.worker.is_none(), "worker is cleared");
    }

    fn lifecycle_result(pr_number: Option<i64>) -> LifecycleResult {
        LifecycleResult {
            session_name: "worker-42-2".to_string(),
            session_id: "sid-new".to_string(),
            branch: "mando/todo-42-7".to_string(),
            worktree: "/tmp/mando-todo-42".to_string(),
            pr_number,
        }
    }

    fn assert_applied_session_fields(item: &Task) {
        assert_eq!(item.worker.as_deref(), Some("worker-42-2"));
        assert_eq!(item.session_ids.worker.as_deref(), Some("sid-new"));
        assert_eq!(item.branch.as_deref(), Some("mando/todo-42-7"));
        assert_eq!(item.worktree.as_deref(), Some("/tmp/mando-todo-42"));
    }

    #[test]
    fn apply_resume_preserves_existing_pr_number() {
        // Resume arm: reopen_worker returns LifecycleResult.pr_number =
        // item.pr_number, and apply_lifecycle_result writes it back. End
        // state keeps the same PR the task was on before reopen.
        let mut item = Task::new("resume");
        item.pr_number = Some(970);

        apply_lifecycle_result(&mut item, lifecycle_result(Some(970)));

        assert_eq!(item.pr_number, Some(970));
        assert_applied_session_fields(&item);
    }

    #[test]
    fn apply_fresh_spawn_replaces_pr_number() {
        // clean_and_spawn_fresh arm: a broken-session recovery minted a
        // new branch + new PR (971). apply_lifecycle_result must overwrite
        // item.pr_number so the row doesn't keep pointing at the abandoned
        // draft PR on the old branch.
        let mut item = Task::new("fresh-spawn");
        item.pr_number = Some(970);

        apply_lifecycle_result(&mut item, lifecycle_result(Some(971)));

        assert_eq!(item.pr_number, Some(971));
        assert_applied_session_fields(&item);
    }

    #[test]
    fn apply_fresh_spawn_create_pr_failure_clears_pr_number() {
        // Fresh-spawn recovery pushed a new branch but create_draft_pr
        // failed (non-fatal; worker still started). Post-op absolute state
        // is "no PR": the old PR is on a dead branch, the new one does not
        // exist. flush_discovered_prs picks it up once the worker pushes.
        let mut item = Task::new("fresh-spawn-pr-failed");
        item.pr_number = Some(970);

        apply_lifecycle_result(&mut item, lifecycle_result(None));

        assert!(item.pr_number.is_none());
        assert_applied_session_fields(&item);
    }
}
