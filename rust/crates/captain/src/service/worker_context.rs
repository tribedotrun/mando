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
}
