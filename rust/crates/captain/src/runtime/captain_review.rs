//! Async, non-blocking captain review sessions.
//!
//! When the classifier decides an item needs CC review, the captain:
//! 1. Spawns a headless CC session (non-blocking)
//! 2. Sets item status to CaptainReviewing
//! 3. On subsequent ticks, polls for completion
//! 4. Applies the verdict (ship/nudge/respawn/escalate/retry)

use anyhow::Result;
use serde::{Deserialize, Serialize};

use settings::CaptainWorkflow;
use settings::Config;

use super::notify::Notifier;
use super::review_phase;
use crate::Task;

#[cfg(test)]
pub(crate) use super::captain_review_check::validate_verdict;
pub(crate) use super::captain_review_check::{check_review, check_review_failed};
pub use super::captain_review_error::handle_review_error;
pub use super::captain_review_verdict::apply_verdict;

/// Structured verdict from a captain review CC session.
///
/// When `action == "ship"`, the verdict MUST also carry `confidence` and
/// `confidence_reason` fields graded against the Confidence section of the
/// `captain_review` prompt: `high` only when every facet was verified against
/// an artifact that was actually opened and a diff hunk that was actually
/// read; anything resting on assumption is `mid`. The mergeability tick
/// consumes `confidence == "high"` as the sole auto-merge gate.
///
/// `Default` is exposed only for test construction via `..Default::default()`;
/// it returns an empty-action verdict which would be rejected by
/// `validate_verdict` in production.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaptainVerdict {
    pub action: String,
    pub feedback: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Set when action = ship. Exactly "high" or "mid" — the same closed set
    /// the enforced JSON schema offers. Anything else coerces to "mid" in
    /// `validate_verdict`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// Set when action = ship. Per problem facet, cites the evidence artifact
    /// that shows it solved and the diff hunk that delivers the fix (or, on a
    /// no-PR task, the workpad).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_reason: Option<String>,
}

/// Allowed actions for a given trigger, matching `is_verdict_allowed`.
fn allowed_actions_for_trigger(trigger: &str) -> &'static [&'static str] {
    // Mirror of `captain_review_check::is_verdict_allowed`. `escalate` is
    // available on every tier. Broken-session reviews are intentionally
    // narrower than the default path: once the session is known dead in
    // place, the captain may ship, respawn, or escalate, but never resume
    // the same session via nudge/reset_budget.
    match trigger {
        "clarifier_fail" => &["retry_clarifier", "escalate"],
        "spawn_fail" => &["respawn", "escalate"],
        "broken_session" => &["ship", "respawn", "escalate"],
        _ => &["ship", "nudge", "respawn", "reset_budget", "escalate"],
    }
}

/// JSON Schema for the CaptainVerdict structured output.
/// Trigger-aware: only offers actions the captain can actually choose.
///
/// `confidence` and `confidence_reason` are always present in the schema
/// (optional) so that `ship` verdicts can carry them. We cannot express
/// "required only when action = ship" in JSON Schema Draft 7, so we rely on
/// prompt instructions and downstream Rust validation (see
/// `captain_review_check::validate_verdict`).
fn verdict_json_schema(trigger: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": allowed_actions_for_trigger(trigger),
                "description": "The verdict action — must be one of the allowed values"
            },
            "feedback": {
                "type": "string",
                "description": "Specific instructions for worker or summary for human"
            },
            "report": {
                "type": "string",
                "description": "CTO-level report, required for escalate"
            },
            "confidence": {
                "type": "string",
                "enum": ["high", "mid"],
                "description": "Required when action = ship. `high` auto-merges with no human look; `mid` stops for one."
            },
            "confidence_reason": {
                "type": "string",
                "description": "Required when action = ship. Per problem facet, cites the evidence artifact that shows it solved and the diff hunk that delivers the fix, then why the grade follows."
            }
        },
        "required": ["action", "feedback"]
    })
}

pub(super) fn codex_verdict_output_schema(
    trigger: &str,
) -> super::agent_runtime::AgentOutputSchema {
    super::agent_runtime::AgentOutputSchema(verdict_json_schema(trigger))
}

/// Spawn a captain review for an item. Sets status to CaptainReviewing.
///
/// The CC session runs async (tokio::spawn) — not awaited here.
/// `db_status` is the status the DB currently has for this task. When callers
/// use `reset_review_retry` before `spawn_review`, they must pass the
/// pre-reset status so the atomic persist guard matches the DB row.
/// When the item is already `CaptainReviewing` in the DB (e.g., tick_review
/// no-session path), pass `None` to use the item's current status.
#[tracing::instrument(skip_all, fields(task_id = item.id, trigger = trigger))]
pub(crate) async fn spawn_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Resolve CWD before mutating item state — if this fails,
    // the item stays in its current status and the caller can retry or escalate.

    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            config
                .captain
                .projects
                .values()
                .next()
                .map(|p| std::path::PathBuf::from(&p.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for captain review: item has no worktree and no projects configured"
            )
        })?;

    // Pre-validate the prompt template exists before committing side effects.
    // Full render happens inside tokio::spawn (needs async evidence/knowledge data),
    // but catching a missing template here prevents stuck CaptainReviewing state.
    if !workflow.prompts.contains_key("captain_review") {
        anyhow::bail!("captain_review prompt template missing from workflow");
    }

    // Parse the trigger up-front: an unknown trigger means a newer component
    // produced a label that captain_review cannot classify, and silently
    // accepting it would cause every such review to escalate with no hint
    // why. Fail the spawn instead.
    let parsed_trigger: crate::ReviewTrigger = trigger
        .parse()
        .map_err(|e| anyhow::anyhow!("captain_review: unknown trigger {trigger:?}: {e}"))?;

    // --- Gather worker context (PR data, stream tail, process status) ---
    // This must run BEFORE any state mutation: build_single_context can fail
    // (e.g. on an unparseable worker_started_at). If we flipped the item to
    // CaptainReviewing first and the context build then errored, callers like
    // action_contract::trigger_review bubble the error up with no rollback,
    // leaving the task stuck in CaptainReviewing with no review session.
    let (_ctx, worker_contexts_text) =
        review_phase::build_single_context(item, config, pool).await?;

    super::agent_runtime::spawn_review_session(
        item,
        trigger,
        db_status,
        cwd,
        parsed_trigger,
        worker_contexts_text,
        workflow,
        notifier,
        pool,
    )
    .await
}

#[cfg(test)]
#[path = "captain_review_tests.rs"]
mod tests;
