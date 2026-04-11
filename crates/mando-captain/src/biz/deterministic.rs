//! Deterministic action classifier. Binary decision tree.
//!
//! 5 decisions: BUDGET -> TIMEOUT -> ACTIVE (skip) -> CC REVIEW -> NUDGE.
//! Every path returns Some(Action); no LLM fallback.

use std::collections::HashMap;

use anyhow::Result;
use mando_types::captain::{Action, ActionKind};
use mando_types::Task;
use mando_types::WorkerContext;
use rustc_hash::FxHashMap;

use super::deterministic_helpers::{
    action, classify_unresolved_threads, has_substantive_output, is_image_dimension_blocked,
    render_nudge,
};
use super::worker_context::{evidence_is_fresh, has_no_evidence, has_summary_diagram};

/// Deterministic classification. Every input shape produces exactly one
/// `Action`; there is no fallthrough. Returns `Err` only when a required
/// nudge template is missing or malformed, so the tick can escalate cleanly
/// instead of panicking.
#[allow(clippy::too_many_arguments)]
pub(crate) fn classify_worker(
    ctx: &WorkerContext,
    item: Option<&Task>,
    stream_result_clean: Option<bool>,
    has_broken_session: bool,
    nudges: &HashMap<String, String>,
    worker_timeout: std::time::Duration,
    stale_threshold: std::time::Duration,
    max_interventions: u32,
) -> Result<Action> {
    let worker_timeout_s = worker_timeout.as_secs_f64();
    let stale_threshold_s = stale_threshold.as_secs_f64();
    let is_no_pr = item.is_some_and(|it| it.no_pr);

    // Rule 0: BUDGET. Intervention budget exhausted -> escalate.
    // Checked first so it always wins as the final backstop, even over timeout.
    if ctx.intervention_count >= max_interventions as i64 {
        return Ok(action(
            ctx,
            ActionKind::CaptainReview,
            "",
            "budget_exhausted",
        ));
    }

    // Rule 1: TIMEOUT. Alive + wall-clock exceeded -> review.
    if worker_timeout_s > 0.0 && ctx.seconds_active >= worker_timeout_s && ctx.process_alive {
        return Ok(action(ctx, ActionKind::CaptainReview, "", "timeout"));
    }

    // Rule 2: ACTIVE. Alive + streaming within threshold -> skip.
    if ctx.process_alive {
        match ctx.stream_stale_s {
            Some(stale) if stale < stale_threshold_s => {
                return Ok(action(ctx, ActionKind::Skip, "", "actively working"));
            }
            None => {
                // No stream file yet; just started, wait for first output.
                return Ok(action(
                    ctx,
                    ActionKind::Skip,
                    "",
                    "waiting for first output",
                ));
            }
            _ => {} // stale >= threshold, fall through to Rule 3/4
        }
    }
    let stream_stale = ctx.stream_stale_s.unwrap_or(f64::MAX);

    // Rule 3: CC REVIEW. Gates pass / broken / budget.
    // Only truly broken sessions (content but no init event) route to captain
    // review here. Dead-but-stale workers fall through to nudge, where the
    // action layer checks for broken sessions before resuming.
    if has_broken_session {
        return Ok(action(ctx, ActionKind::CaptainReview, "", "broken_session"));
    }
    // budget_exhausted is now checked in Rule 0 (before timeout).
    if ctx.degraded && ctx.pr.is_some() && stream_result_clean == Some(true) {
        return Ok(action(
            ctx,
            ActionKind::CaptainReview,
            "",
            "degraded_context",
        ));
    }
    if quality_gates_pass(ctx, is_no_pr, stream_result_clean) {
        return Ok(action(ctx, ActionKind::CaptainReview, "", "gates_pass"));
    }

    // Rule 4: NUDGE. Worker needs a push.

    // Check specific gate failures first (work done but missing something).
    if let Some(gate_nudge) = missing_gate_nudge(ctx, is_no_pr, stream_result_clean, nudges)? {
        return Ok(gate_nudge);
    }

    // Process alive but stale.
    if ctx.process_alive && stream_stale >= stale_threshold_s {
        let stale_str = format!("{:.0}", stream_stale);
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("stale_s", stale_str.as_str());
        let msg = render_nudge(nudges, "stream_stale", &vars)?;
        return Ok(action(ctx, ActionKind::Nudge, &msg, "you appear stuck"));
    }

    // Process dead or alive fallback. Diagnose which gates are failing.
    let diagnosis = diagnose_failing_gates(ctx, is_no_pr, stream_result_clean);
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("failures", diagnosis.as_str());
    let msg = render_nudge(nudges, "gates_incomplete", &vars)?;
    let reason = format!("gates incomplete: {diagnosis}");
    Ok(action(ctx, ActionKind::Nudge, &msg, &reason))
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// All quality gates pass, work looks complete.
///
/// Explicitly refuses to pass gates when `ctx.degraded` is set: a degraded
/// PR fetch means the captain cannot trust its own hygiene counters
/// (unresolved_threads / unreplied_threads / unaddressed_issue_comments
/// may all still be 0 because the fetch failed, not because the PR is clean).
/// The classifier routes degraded items to CaptainReview via a separate
/// rule, so this guard ensures we never ship past that rule via the
/// gates-pass path.
fn quality_gates_pass(
    ctx: &WorkerContext,
    is_no_pr: bool,
    stream_result_clean: Option<bool>,
) -> bool {
    // Degraded PR data must never pass gates: the hygiene counters cannot
    // be trusted when the underlying GitHub fetch partially failed.
    if ctx.degraded {
        return false;
    }
    // Must have a clean stream result to consider gates passed.
    if stream_result_clean != Some(true) {
        return false;
    }
    // PR path.
    if ctx.pr.is_some()
        && ctx.branch_ahead
        && ctx.has_reopen_ack
        && ctx.unresolved_threads == 0
        && ctx.unreplied_threads == 0
        && ctx.unaddressed_issue_comments == 0
        && has_summary_diagram(ctx)
        && !has_no_evidence(&ctx.pr_body)
        && evidence_is_fresh(ctx)
    {
        return true;
    }
    // No-PR path.
    if is_no_pr && has_substantive_output(&ctx.stream_tail) && ctx.seconds_active >= 180.0 {
        return true;
    }
    false
}

/// List every failing quality gate for the diagnostic nudge message.
fn diagnose_failing_gates(
    ctx: &WorkerContext,
    is_no_pr: bool,
    stream_result_clean: Option<bool>,
) -> String {
    let mut failures: Vec<String> = Vec::new();

    if stream_result_clean != Some(true) {
        failures.push("no clean stream result".into());
    }
    if !is_no_pr {
        if ctx.pr.is_none() {
            failures.push("no PR created — push your branch and open a PR".into());
        }
        if ctx.pr.is_some() && !ctx.branch_ahead {
            failures.push("branch not ahead of main".into());
        }
        if ctx.pr.is_some() && !has_summary_diagram(ctx) {
            failures.push("missing PR summary diagram".into());
        }
        if ctx.pr.is_some() && has_no_evidence(&ctx.pr_body) {
            failures.push("missing evidence in PR".into());
        }
        if ctx.pr.is_some() && !has_no_evidence(&ctx.pr_body) && !evidence_is_fresh(ctx) {
            failures.push("stale evidence -- recapture after reopen".into());
        }
        if ctx.unresolved_threads > 0 {
            failures.push(format!("{} unresolved thread(s)", ctx.unresolved_threads));
        }
        if ctx.unreplied_threads > 0 {
            failures.push(format!("{} unreplied thread(s)", ctx.unreplied_threads));
        }
        if ctx.unaddressed_issue_comments > 0 {
            failures.push(format!(
                "{} unaddressed comment(s)",
                ctx.unaddressed_issue_comments
            ));
        }
        if ctx.reopen_seq > 0 && !ctx.has_reopen_ack {
            failures.push(format!("reopen #{} not acknowledged", ctx.reopen_seq));
        }
    }
    if is_no_pr {
        if !has_substantive_output(&ctx.stream_tail) {
            failures.push("insufficient output (< 20 chars)".into());
        }
        if ctx.seconds_active < 180.0 {
            failures.push("insufficient runtime (< 3 min)".into());
        }
    }

    if failures.is_empty() {
        "unknown gate failure".into()
    } else {
        failures.join("; ")
    }
}

/// When work has a stream result but gates fail, produce a specific nudge.
fn missing_gate_nudge(
    ctx: &WorkerContext,
    is_no_pr: bool,
    stream_result_clean: Option<bool>,
    nudges: &HashMap<String, String>,
) -> Result<Option<Action>> {
    // Only applies when we have a stream result (work reached a conclusion).
    if stream_result_clean.is_none() {
        return Ok(None);
    }

    let vars: FxHashMap<&str, &str> = FxHashMap::default();

    // Threads.
    if !ctx.degraded
        && ctx.pr.is_some()
        && (ctx.unresolved_threads > 0
            || ctx.unreplied_threads > 0
            || ctx.unaddressed_issue_comments > 0)
    {
        return Ok(Some(classify_unresolved_threads(ctx, nudges)?));
    }
    // Diagram.
    if !ctx.degraded && ctx.pr.is_some() && !has_summary_diagram(ctx) {
        let msg = render_nudge(nudges, "missing_diagram", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "PR missing summary diagram",
        )));
    }
    // Evidence.
    if !ctx.degraded && ctx.pr.is_some() && has_no_evidence(&ctx.pr_body) {
        let msg = render_nudge(nudges, "missing_evidence", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "PR missing evidence",
        )));
    }
    // Reopen ack (before stale evidence -- worker must address feedback first).
    if !ctx.degraded && ctx.pr.is_some() && !ctx.has_reopen_ack && ctx.reopen_seq > 0 {
        let source = ctx.reopen_source.as_deref();
        let (ack_prefix, context_file, source_label) = match source {
            Some("review") => (
                "Review-Reopen",
                "captain-reopen-context.md",
                "review threads",
            ),
            Some("ci") => ("CI-Reopen", "captain-reopen-context.md", "CI failures"),
            Some("evidence") => (
                "Evidence-Reopen",
                "captain-reopen-context.md",
                "missing evidence",
            ),
            _ => ("Reopen", "captain-reopen-context.md", "human feedback"),
        };
        let seq_str = ctx.reopen_seq.to_string();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("source_label", source_label);
        vars.insert("reopen_seq", seq_str.as_str());
        vars.insert("context_file", context_file);
        vars.insert("ack_prefix", ack_prefix);
        let msg = render_nudge(nudges, "reopen_ack", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            &format!("reopen #{} pending", ctx.reopen_seq),
        )));
    }
    // Stale evidence (exists but not fresh after reopen).
    if !ctx.degraded
        && ctx.pr.is_some()
        && !has_no_evidence(&ctx.pr_body)
        && !evidence_is_fresh(ctx)
    {
        let msg = render_nudge(nudges, "stale_evidence", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "PR evidence stale after reopen",
        )));
    }
    // Image dimension blocked.
    if is_image_dimension_blocked(&ctx.stream_tail) {
        let msg = render_nudge(nudges, "image_dimension_blocked", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "image dimension blocked",
        )));
    }
    // No-PR insufficient output.
    if is_no_pr && !has_substantive_output(&ctx.stream_tail) {
        let msg = render_nudge(nudges, "nopr_insufficient_output", &vars)?;
        return Ok(Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "insufficient output",
        )));
    }
    Ok(None)
}

#[cfg(test)]
#[path = "deterministic_tests.rs"]
mod tests;
