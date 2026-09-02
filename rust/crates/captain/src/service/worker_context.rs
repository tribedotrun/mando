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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> WorkerContext {
        WorkerContext {
            session_name: "mando-worker-0".into(),
            item_title: "Test task".into(),
            status: "in-progress".into(),
            branch: Some("feature/test".into()),
            pr: Some("#42".into()),
            pr_ci_status: Some("success".into()),
            pr_comments: 2,
            unresolved_threads: 0,
            unreplied_threads: 0,
            unaddressed_issue_comments: 0,
            pr_body: String::new(),
            changed_files: vec![],
            pr_is_draft: false,
            branch_ahead: true,
            process_alive: false,
            cpu_time_s: Some(100.0),
            prev_cpu_time_s: Some(90.0),
            stream_tail: "done".into(),
            seconds_active: 5400.0,
            intervention_count: 0,
            no_pr: false,
            reopen_seq: 0,
            has_reopen_ack: true,
            reopen_source: None,
            stream_stale_s: None,
            pr_head_sha: "abc123".into(),
            degraded: false,
            has_evidence: false,
            evidence_fresh: false,
            has_work_summary: false,
            work_summary_fresh: false,
            has_screenshot: false,
            has_recording: false,
            has_cannot_reproduce: false,
        }
    }

    #[test]
    fn summary_missing_when_no_db_artifacts() {
        let ctx = make_ctx();
        assert!(!has_summary_diagram(&ctx));
    }

    #[test]
    fn summary_present_and_fresh() {
        let mut ctx = make_ctx();
        ctx.has_work_summary = true;
        ctx.work_summary_fresh = true;
        assert!(has_summary_diagram(&ctx));
    }

    #[test]
    fn summary_stale_after_reopen() {
        let mut ctx = make_ctx();
        ctx.has_work_summary = true;
        ctx.work_summary_fresh = false;
        assert!(!has_summary_diagram(&ctx));
    }

    #[test]
    fn evidence_fresh_field() {
        let mut ctx = make_ctx();
        ctx.evidence_fresh = true;
        assert!(ctx.evidence_fresh);
    }

    #[test]
    fn evidence_not_fresh_by_default() {
        let ctx = make_ctx();
        assert!(!ctx.evidence_fresh);
    }

    #[test]
    fn evidence_status_missing_when_no_artifacts() {
        let ctx = make_ctx();
        assert_eq!(evidence_status(&ctx), "MISSING");
    }

    #[test]
    fn evidence_status_present_when_fresh() {
        let mut ctx = make_ctx();
        ctx.has_evidence = true;
        ctx.evidence_fresh = true;
        assert_eq!(evidence_status(&ctx), "present");
    }

    #[test]
    fn evidence_status_stale_after_reopen() {
        let mut ctx = make_ctx();
        ctx.has_evidence = true;
        ctx.evidence_fresh = false;
        assert_eq!(
            evidence_status(&ctx),
            "STALE (evidence exists but predates reopen)"
        );
    }

    #[test]
    fn format_produces_output() {
        let ctx = make_ctx();
        let formatted = format_context(&ctx);
        assert!(formatted.contains("### Worker: mando-worker-0"));
        assert!(formatted.contains("Item: Test task"));
    }

    // ── UI capture gate ──

    fn ctx_with_captures(screenshot: bool, recording: bool) -> WorkerContext {
        let mut ctx = make_ctx();
        ctx.has_evidence = true;
        ctx.evidence_fresh = true;
        ctx.has_screenshot = screenshot;
        ctx.has_recording = recording;
        ctx
    }

    #[test]
    fn ui_gap_none_with_screenshot_and_recording() {
        let ctx = ctx_with_captures(true, true);
        assert_eq!(ui_evidence_gap(&ctx), None);
    }

    #[test]
    fn ui_gap_open_without_recording() {
        // A png alone proves an end state, not the action; the deck still
        // owes a recording.
        let ctx = ctx_with_captures(true, false);
        assert_eq!(ui_evidence_gap(&ctx), Some(EvidenceGap::Missing));
    }

    #[test]
    fn ui_gap_open_without_screenshot() {
        let ctx = ctx_with_captures(false, true);
        assert_eq!(ui_evidence_gap(&ctx), Some(EvidenceGap::Missing));
    }

    #[test]
    fn ui_gap_never_asks_for_a_before_capture() {
        // Captures carry no kind tag at all and still close the gate: there
        // is no before/after pairing requirement anywhere in the gate.
        let mut ctx = ctx_with_captures(true, true);
        ctx.has_cannot_reproduce = false;
        assert_eq!(ui_evidence_gap(&ctx), None);
    }

    #[test]
    fn gap_reads_stale_when_evidence_predates_reopen() {
        let mut ctx = ctx_with_captures(false, false);
        ctx.has_evidence = true;
        ctx.evidence_fresh = false;
        assert_eq!(ui_evidence_gap(&ctx), Some(EvidenceGap::Stale));
        assert_eq!(ui_evidence_gap(&ctx).unwrap().nudge_key(), "stale_evidence");
    }

    #[test]
    fn gap_reads_missing_when_no_evidence_at_all() {
        let mut ctx = make_ctx();
        ctx.has_evidence = false;
        ctx.evidence_fresh = false;
        assert_eq!(ui_evidence_gap(&ctx), Some(EvidenceGap::Missing));
        assert_eq!(
            ui_evidence_gap(&ctx).unwrap().nudge_key(),
            "missing_evidence"
        );
    }

    #[test]
    fn touches_ui_detects_frontend_extensions() {
        let mut ctx = make_ctx();
        ctx.changed_files = vec!["electron/src/renderer/ui/Panel.tsx".into()];
        assert!(touches_ui(&ctx));
        ctx.changed_files = vec!["electron/src/renderer/ui/panel.CSS".into()];
        assert!(touches_ui(&ctx), "extension match is case-insensitive");
    }

    #[test]
    fn touches_ui_false_for_backend_only_and_empty_diffs() {
        let mut ctx = make_ctx();
        ctx.changed_files = vec!["rust/crates/captain/src/service/deterministic.rs".into()];
        assert!(!touches_ui(&ctx));
        // Degraded / pre-PR fetches produce an empty list; it must not
        // manufacture a UI evidence requirement.
        ctx.changed_files = vec![];
        assert!(!touches_ui(&ctx));
    }
}
