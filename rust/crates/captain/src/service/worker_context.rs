//! WorkerContext builder -- computed properties for captain review.
//!
//! Gate functions now use DB-backed artifact fields on WorkerContext
//! (has_evidence, evidence_fresh, has_work_summary, work_summary_fresh)
//! instead of parsing PR body text.

use crate::WorkerContext;

/// Check if task has a work summary (DB-backed).
pub(crate) fn has_summary_diagram(ctx: &WorkerContext) -> bool {
    ctx.has_work_summary && ctx.work_summary_fresh
}

/// Which evidence gate a task is failing, and therefore which existing
/// nudge template answers it.
///
/// The `captain_review` prompt does not ask the reviewer to check evidence
/// shape — it states that these gates fire deterministically before review.
/// These predicates are that determinism: they run on the classifier's nudge
/// path so a `gates_pass` review cannot fire on an incomplete deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceGap {
    /// No capture of the required sort exists at all.
    Missing,
    /// Captures exist but predate the latest reopen.
    Stale,
}

impl EvidenceGap {
    /// The `nudges:` key in `captain-workflow.yaml` that addresses this gap.
    /// Both keys already exist; no new nudge template is introduced.
    pub(crate) fn nudge_key(self) -> &'static str {
        match self {
            Self::Missing => "missing_evidence",
            Self::Stale => "stale_evidence",
        }
    }

    pub(crate) fn reason(self, what: &str) -> String {
        match self {
            Self::Missing => format!("missing {what}"),
            Self::Stale => format!("stale {what} — recapture after reopen"),
        }
    }
}

/// UI work needs a fresh screenshot of the end state and a fresh recording of
/// the action. No "before" capture is required: the diff already shows what
/// changed, and a baseline of the old behavior never proves the new one
/// works. Returns `None` once both are present and fresh.
///
/// `evidence_fresh` distinguishes "never captured" from "captured before the
/// reopen": when the task has evidence that simply went stale, the stale-
/// evidence nudge is the honest message.
pub(crate) fn ui_evidence_gap(ctx: &WorkerContext) -> Option<EvidenceGap> {
    if ctx.has_screenshot && ctx.has_recording {
        return None;
    }
    Some(if ctx.has_evidence && !ctx.evidence_fresh {
        EvidenceGap::Stale
    } else {
        EvidenceGap::Missing
    })
}

/// True when the task's diff touches UI. The capture gate only binds UI work,
/// so a backend-only change is not held to screenshot + recording.
///
/// Conservative on purpose: `changed_files` is empty when the PR fetch
/// degraded or no PR exists yet, and an empty list must not manufacture a UI
/// requirement out of nothing.
pub(crate) fn touches_ui(ctx: &WorkerContext) -> bool {
    const UI_EXTS: &[&str] = &[".tsx", ".jsx", ".vue", ".svelte", ".css", ".scss", ".html"];
    ctx.changed_files.iter().any(|f| {
        let lower = f.to_lowercase();
        UI_EXTS.iter().any(|ext| lower.ends_with(ext))
    })
}

/// Classify evidence status for captain review context (DB-backed).
pub(crate) fn evidence_status(ctx: &WorkerContext) -> &'static str {
    if !ctx.has_evidence {
        return "MISSING";
    }
    if !ctx.evidence_fresh {
        return "STALE (evidence exists but predates reopen)";
    }
    "present"
}

/// Format a WorkerContext for LLM captain review input.
pub(crate) fn format_context(ctx: &WorkerContext) -> String {
    let evidence_section = evidence_status(ctx);
    let stream_stale = match ctx.stream_stale_s {
        Some(s) => format!("{:.0}s", s),
        None => "n/a".to_string(),
    };
    let stream_tail_snippet = if ctx.stream_tail.len() > 500 {
        let start = ctx.stream_tail.len() - 500;
        // Find nearest char boundary at or after `start` to avoid panic on multi-byte UTF-8.
        let safe_start = ctx.stream_tail.ceil_char_boundary(start);
        &ctx.stream_tail[safe_start..]
    } else {
        &ctx.stream_tail
    };

    format!(
        "### Worker: {name}\n\
         - Item: {title}\n\
         - Status: {status}\n\
         - Branch: {branch}\n\
         - PR: {pr}\n\
         - PR draft: {draft}\n\
         - CI: {ci}\n\
         - PR comments: {comments} top-level, {unresolved} unresolved threads, \
           {unreplied} unreplied threads, {unaddressed} unaddressed issue comments\n\
         - Summary diagram in PR: {diagram}\n\
         - Evidence in PR: {evidence}\n\
         - Branch ahead of main: {ahead}\n\
         - Process alive: {alive}\n\
         - CPU time: {cpu}s (prev: {prev_cpu}s)\n\
         - Seconds active: {seconds_active} ({hours:.1}h)\n\
         - Crash count: {crash}\n\
         - no_pr: {no_pr}\n\
         - Reopen seq: {reopen_seq}\n\
         - Reopen source: {reopen_source}\n\
         - Has reopen ack: {reopen_ack}\n\
         - Stream stale: {stream}\n\
         - **DEGRADED**: {degraded}\n\
         - Last output:\n\
         ```\n{tail}\n```",
        name = ctx.session_name,
        title = ctx.item_title,
        status = ctx.status,
        branch = ctx.branch.as_deref().unwrap_or("none"),
        pr = ctx.pr.as_deref().unwrap_or("none"),
        draft = ctx.pr_is_draft,
        ci = ctx.pr_ci_status.as_deref().unwrap_or("n/a"),
        comments = ctx.pr_comments,
        unresolved = ctx.unresolved_threads,
        unreplied = ctx.unreplied_threads,
        unaddressed = ctx.unaddressed_issue_comments,
        diagram = has_summary_diagram(ctx),
        evidence = evidence_section,
        ahead = ctx.branch_ahead,
        alive = ctx.process_alive,
        cpu = ctx
            .cpu_time_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "None".into()),
        prev_cpu = ctx
            .prev_cpu_time_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "None".into()),
        seconds_active = ctx.seconds_active,
        hours = ctx.seconds_active / 3600.0,
        crash = ctx.intervention_count,
        no_pr = ctx.no_pr,
        reopen_seq = ctx.reopen_seq,
        reopen_source = ctx.reopen_source.as_deref().unwrap_or("n/a"),
        reopen_ack = ctx.has_reopen_ack,
        stream = stream_stale,
        degraded = ctx.degraded,
        tail = stream_tail_snippet,
    )
}
