//! Deterministic action classifier -- binary decision tree.
//!
//! 4 decisions: TIMEOUT -> ACTIVE (skip) -> CC REVIEW -> NUDGE.
//! Every path returns Some(Action) -- no LLM fallback.

use std::collections::HashMap;

use mando_types::captain::{Action, ActionKind};
use mando_types::Task;
use mando_types::WorkerContext;

use super::deterministic_helpers::{
    action, classify_unresolved_threads, has_substantive_output, is_image_dimension_blocked,
    render_nudge,
};
use super::worker_context::{has_no_evidence, has_summary_diagram};

/// Deterministic classification. Always returns `Some(Action)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn classify_worker(
    ctx: &WorkerContext,
    item: Option<&Task>,
    stream_result_clean: Option<bool>,
    has_broken_session: bool,
    nudges: &HashMap<String, String>,
    worker_timeout_s: f64,
    stale_threshold_s: f64,
    max_interventions: u32,
) -> Option<Action> {
    let is_no_pr = item.is_some_and(|it| it.no_pr);

    // ── Rule 1: TIMEOUT — alive + wall-clock exceeded → review ──
    if worker_timeout_s > 0.0 && ctx.seconds_active >= worker_timeout_s && ctx.process_alive {
        return Some(action(ctx, ActionKind::CaptainReview, "", "timeout"));
    }

    // ── Rule 2: ACTIVE — alive + streaming within threshold → skip ──
    if ctx.process_alive {
        match ctx.stream_stale_s {
            Some(stale) if stale < stale_threshold_s => {
                return Some(action(ctx, ActionKind::Skip, "", "actively working"));
            }
            None => {
                // No stream file yet — just started, wait for first output.
                return Some(action(
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

    // ── Rule 3: CC REVIEW — gates pass / broken / budget ──
    if quality_gates_pass(ctx, is_no_pr, stream_result_clean) {
        return Some(action(ctx, ActionKind::CaptainReview, "", "gates_pass"));
    }
    let is_broken = has_broken_session
        || (stream_result_clean.is_none() && !ctx.process_alive && stream_stale > 30.0);
    if is_broken {
        return Some(action(ctx, ActionKind::CaptainReview, "", "broken_session"));
    }
    if ctx.intervention_count >= max_interventions as i64 {
        return Some(action(
            ctx,
            ActionKind::CaptainReview,
            "",
            "budget_exhausted",
        ));
    }

    // ── Rule 4: NUDGE — worker needs a push ──

    // Check specific gate failures first (work done but missing something).
    if let Some(gate_nudge) = missing_gate_nudge(ctx, is_no_pr, stream_result_clean, nudges) {
        return Some(gate_nudge);
    }

    // Process alive but stale.
    if ctx.process_alive && stream_stale >= stale_threshold_s {
        let stale_str = format!("{:.0}", stream_stale);
        let mut vars = HashMap::new();
        vars.insert("stale_s", stale_str.as_str());
        let msg = render_nudge(nudges, "stream_stale", &vars);
        return Some(action(ctx, ActionKind::Nudge, &msg, "you appear stuck"));
    }

    // Process dead, work not done.
    if !ctx.process_alive {
        return Some(action(ctx, ActionKind::Nudge, "", "continue working"));
    }

    // Fallback: alive, no stream data yet, not stale (stream_stale_s was None → MAX).
    Some(action(ctx, ActionKind::Nudge, "", "continue working"))
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// All quality gates pass — work looks complete.
fn quality_gates_pass(
    ctx: &WorkerContext,
    is_no_pr: bool,
    stream_result_clean: Option<bool>,
) -> bool {
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
    {
        return true;
    }
    // No-PR path.
    if is_no_pr && has_substantive_output(&ctx.stream_tail) && ctx.seconds_active >= 180.0 {
        return true;
    }
    false
}

/// When work has a stream result but gates fail, produce a specific nudge.
fn missing_gate_nudge(
    ctx: &WorkerContext,
    is_no_pr: bool,
    stream_result_clean: Option<bool>,
    nudges: &HashMap<String, String>,
) -> Option<Action> {
    // Only applies when we have a stream result (work reached a conclusion).
    stream_result_clean?;

    let vars = HashMap::new();

    // Threads.
    if ctx.pr.is_some()
        && (ctx.unresolved_threads > 0
            || ctx.unreplied_threads > 0
            || ctx.unaddressed_issue_comments > 0)
    {
        return Some(classify_unresolved_threads(ctx, nudges));
    }
    // Diagram.
    if ctx.pr.is_some() && !has_summary_diagram(ctx) {
        let msg = render_nudge(nudges, "missing_diagram", &vars);
        return Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "PR missing summary diagram",
        ));
    }
    // Evidence.
    if ctx.pr.is_some() && has_no_evidence(&ctx.pr_body) {
        let msg = render_nudge(nudges, "missing_evidence", &vars);
        return Some(action(ctx, ActionKind::Nudge, &msg, "PR missing evidence"));
    }
    // Reopen ack.
    if ctx.pr.is_some() && !ctx.has_reopen_ack && ctx.reopen_seq > 0 {
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
        let mut vars = HashMap::new();
        vars.insert("source_label", source_label);
        vars.insert("reopen_seq", seq_str.as_str());
        vars.insert("context_file", context_file);
        vars.insert("ack_prefix", ack_prefix);
        let msg = render_nudge(nudges, "reopen_ack", &vars);
        return Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            &format!("reopen #{} pending", ctx.reopen_seq),
        ));
    }
    // Image dimension blocked.
    if is_image_dimension_blocked(&ctx.stream_tail) {
        let msg = render_nudge(nudges, "image_dimension_blocked", &vars);
        return Some(action(
            ctx,
            ActionKind::Nudge,
            &msg,
            "image dimension blocked",
        ));
    }
    // No-PR insufficient output.
    if is_no_pr && !has_substantive_output(&ctx.stream_tail) {
        let msg = render_nudge(nudges, "nopr_insufficient_output", &vars);
        return Some(action(ctx, ActionKind::Nudge, &msg, "insufficient output"));
    }
    None
}

#[cfg(test)]
#[path = "deterministic_tests.rs"]
mod tests;
