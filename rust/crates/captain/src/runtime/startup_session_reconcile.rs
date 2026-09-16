//! Startup reconciliation for in-flight CC sessions.
//!
//! On daemon restart, any `cc_sessions` row still marked `running` belongs
//! to a subprocess that either never existed after restart or was an
//! orphan killed by `pid_registry::cleanup_on_startup`. This module walks
//! every such row, decides whether the session produced a usable stream
//! result, and calls `terminate_session` with the appropriate terminal
//! status. Task-side salvage (promoting a clarifier/review/merge result
//! into task state) happens automatically on the first captain tick,
//! which already reads stream files via `tick_clarify_poll`,
//! `captain_review_check`, and `captain_merge_poll`.
//!
//! Expected behaviour after this runs:
//! - No `cc_sessions` row remains in `running` state.
//! - Sessions whose stream ends with a clean `result` event are marked
//!   `Stopped` with cost/duration backfilled.
//! - Interrupted results are also marked `Stopped`, but are not considered
//!   clean or salvageable task output.
//! - Sessions whose stream has an error or no result at all are marked
//!   `Failed`.
//! - First captain tick (<=60s later) salvages stopped-with-result
//!   sessions into their parent tasks. Failed sessions trigger a fresh
//!   clean-slate respawn on the same tick.
//! - Tasks stranded in `Clarifying` from a prior daemon run are reverted
//!   to `NeedsClarification` so the human can re-answer the outstanding
//!   question. This replaces what the deleted `dispatch_reclarify`
//!   tick-path safety net used to handle during normal operation.

use anyhow::{Context, Result};
use api_types::TimelineEventPayload;
use global_types::SessionStatus;
use sqlx::SqlitePool;

use crate::io::session_terminate::terminate_session;
use crate::service::lifecycle;
use crate::{ItemStatus, TimelineEvent};

/// Reconcile every session currently marked `running`.
///
/// Returns an error when the enumerating query itself fails so the
/// caller (captain's `reconcile_on_startup`) can propagate via `?`
/// and let the daemon's `MANDO_UNSAFE_START` gate decide whether to
/// abort boot. Per-row `terminate_session` calls do their own
/// structured logging on failure and do not stop the loop; a single
/// corrupt stream file should not block recovery of the other rows.
///
/// Assumes `pid_registry::cleanup_on_startup` has already run so PID
/// kill inside `terminate_session` is effectively a no-op.
#[tracing::instrument(skip_all)]
pub async fn reconcile_startup_sessions(pool: &SqlitePool) -> Result<()> {
    let running = sessions_db::list_running_sessions(pool)
        .await
        .context("startup session reconciliation: list_running_sessions failed")?;
    if running.is_empty() {
        return Ok(());
    }

    let mut salvaged = 0u32;
    let mut interrupted = 0u32;
    let mut failed = 0u32;
    for row in &running {
        let sid = row.session_id.as_str();
        let stream_path = super::agent_runtime::stream_path(row.provider, sid);
        let status = match global_claude::get_stream_result(&stream_path)
            .map(|result| global_claude::result_outcome(&result))
        {
            Some(global_claude::ResultOutcome::Success) => {
                salvaged += 1;
                SessionStatus::Stopped
            }
            Some(global_claude::ResultOutcome::Interrupted) => {
                interrupted += 1;
                SessionStatus::Stopped
            }
            _ => {
                failed += 1;
                SessionStatus::Failed
            }
        };
        terminate_session(pool, sid, status, None).await;
    }

    tracing::info!(
        module = "startup",
        total = running.len(),
        salvaged,
        interrupted,
        failed,
        "reconciled in-flight sessions from prior daemon"
    );
    Ok(())
}

/// Unstick any task stranded in `Clarifying` from a prior daemon run.
///
/// With the `dispatch_reclarify` safety net removed and
/// `persist_resume_clarifier` no longer nulling the clarifier session id,
/// the only way a task ends a daemon lifetime in `Clarifying` is a daemon
/// crash during the HTTP inline reclarifier call. The stream file for its
/// clarifier session either never completed or carries a result that has
/// already been applied to the task. Either way, the task is not reachable
/// by any live writer after restart:
///
/// - `tick_clarify_poll` skips sessions whose `result_applied_at` is set
///   (idempotency guard), so it will not re-apply the prior round's
///   already-consumed stream on top of the unanswered human turn.
/// - Nothing else polls or dispatches follow-up clarifiers.
///
/// This function walks those tasks once and reverts them to
/// `NeedsClarification`. The outstanding question from the prior
/// `ClarifyQuestion` timeline event is still visible in the UI, so the
/// human can resend their answer and the HTTP inline path takes over
/// cleanly. Logs and a `ClarifierFailed` timeline event record the
/// recovery for postmortem.
///
/// Non-fatal: a single task that fails to revert is logged and the loop
/// continues. Daemon boot is not blocked on a corrupt row.
#[tracing::instrument(skip_all)]
pub async fn reconcile_stranded_clarifying_tasks(pool: &SqlitePool) {
    let tasks = match crate::io::queries::tasks::load_all(pool).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(
                module = "startup",
                error = %e,
                "failed to load tasks — skipping stranded-clarifier reconcile"
            );
            return;
        }
    };

    let mut recovered = 0u32;
    for task in tasks {
        if task.status != ItemStatus::Clarifying {
            continue;
        }

        // Classify why this task is stranded so the recovery log line is
        // useful. Three shapes are possible in practice:
        //   (a) no clarifier session id — legacy DB from before this PR,
        //       or a previous writer nulled it and crashed;
        //   (b) session has `result_applied_at` set — daemon died after a
        //       successful prior round's apply and before the next CC call
        //       returned;
        //   (c) session exists but has no applied marker — the prior
        //       `reconcile_startup_sessions` has already set the row to
        //       Stopped or Failed; the next `tick_clarify_poll` will read
        //       the stream file and apply/revert accordingly, so skip here.
        //       A transient DB error loading the cc_sessions row leaves the
        //       task stuck until the next restart (unrecoverable from a
        //       persistently-broken schema; recoverable from a transient
        //       blip because tick_clarify_poll retries on every tick).
        let reason = match task.session_ids.clarifier.as_deref() {
            None => "no clarifier session id",
            Some(sid) => match sessions_db::session_by_id(pool, sid).await {
                Ok(Some(row)) if row.result_applied_at.is_some() => {
                    "prior round's result already applied"
                }
                Ok(Some(_)) => {
                    // Session result not yet applied — tick_clarify_poll
                    // will pick up the stream file and process it.
                    continue;
                }
                Ok(None) => "session id points to missing cc_sessions row",
                Err(e) => {
                    tracing::warn!(
                        module = "startup",
                        task_id = task.id,
                        %sid,
                        error = %e,
                        "failed to load cc_sessions row for stranded-clarifier reconcile \
                         — task stays in Clarifying until next tick's retry or next restart"
                    );
                    continue;
                }
            },
        };

        if let Err(e) = revert_stranded_task_to_needs_clarification(pool, &task, reason).await {
            tracing::warn!(
                module = "startup",
                task_id = task.id,
                error = %e,
                reason,
                "failed to revert stranded Clarifying task — leaving in place for manual recovery"
            );
        } else {
            recovered += 1;
        }
    }

    if recovered > 0 {
        tracing::info!(
            module = "startup",
            recovered,
            "recovered stranded Clarifying tasks (reverted to NeedsClarification)"
        );
    }
}

async fn revert_stranded_task_to_needs_clarification(
    pool: &SqlitePool,
    task: &crate::Task,
    reason: &str,
) -> Result<()> {
    let mut next = task.clone();
    lifecycle::apply_clarifier_failure(&mut next)?;
    let message = format!("Stranded clarifier recovered after daemon restart ({reason})");
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "startup".to_string(),
        summary: message.clone(),
        data: TimelineEventPayload::ClarifierFailed {
            session_id: task.session_ids.clarifier.clone().unwrap_or_default(),
            api_error_status: 0,
            message,
        },
    };
    let applied = crate::io::queries::tasks::persist_status_transition_with_command(
        pool,
        &next,
        ItemStatus::Clarifying.as_str(),
        "needs_clarification",
        &event,
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_status_transition_with_command rejected revert for task {}",
        task.id
    );
    Ok(())
}
