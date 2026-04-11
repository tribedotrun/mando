//! WorkerContext builder — computed properties for captain review.

use mando_types::WorkerContext;

use crate::pr_evidence::{evidence_sections, html_img_src_urls};

/// Check if PR body contains a PR Summary section.
///
/// After a reopen, the diagram must be fresh (SHA marker matches HEAD).
pub(crate) fn has_summary_diagram(ctx: &WorkerContext) -> bool {
    if ctx.pr_body.is_empty()
        || !(ctx.pr_body.contains("## PR Summary") || ctx.pr_body.contains("## Summary Diagram"))
    {
        return false;
    }
    if ctx.reopen_seq > 0 && !ctx.pr_head_sha.is_empty() {
        return summary_diagram_is_fresh(ctx);
    }
    true
}

/// Check if evidence-head marker in PR body matches the branch HEAD.
///
/// Returns true when no reopen has happened (freshness only matters after reopen).
/// After a reopen, evidence must have a `<!-- evidence-head: <sha> -->` marker
/// matching the current HEAD.
pub(crate) fn evidence_is_fresh(ctx: &WorkerContext) -> bool {
    if ctx.reopen_seq == 0 || ctx.pr_head_sha.is_empty() {
        return true;
    }
    let marker = "<!-- evidence-head: ";
    let mut last_sha: Option<&str> = None;

    for line in ctx.pr_body.lines() {
        if let Some(rest) = line.trim().strip_prefix(marker) {
            if let Some(sha) = rest.strip_suffix(" -->") {
                last_sha = Some(sha);
            }
        }
    }

    match last_sha {
        Some(sha) if !sha.is_empty() => ctx.pr_head_sha.starts_with(sha),
        _ => false,
    }
}

/// Check if pr-summary-head marker in PR body matches the branch HEAD.
fn summary_diagram_is_fresh(ctx: &WorkerContext) -> bool {
    let marker = "<!-- pr-summary-head: ";
    let mut last_sha: Option<&str> = None;

    for line in ctx.pr_body.lines() {
        if let Some(rest) = line.trim().strip_prefix(marker) {
            if let Some(sha) = rest.strip_suffix(" -->") {
                last_sha = Some(sha);
            }
        }
    }

    match last_sha {
        Some(sha) => ctx.pr_head_sha.starts_with(sha),
        None => false,
    }
}

/// True when PR body definitely has no evidence.
///
/// Checks for anchored heading markers that indicate real evidence,
/// AND verifies there is substantive content (image URL or code block)
/// beneath the heading — not just an empty heading.
pub(crate) fn has_no_evidence(pr_body: &str) -> bool {
    // Empty body means either no PR or fetch failure — not "missing evidence".
    // Callers should treat empty body as inconclusive, not as a gate failure.
    if pr_body.is_empty() {
        return false;
    }
    let sections = evidence_sections(pr_body);
    if sections.is_empty() {
        return true;
    }
    !sections.into_iter().any(section_has_substantive_evidence)
}

/// Check if PR body has substantive evidence content (not just a heading).
///
/// Looks for image/video URLs or code blocks that indicate actual runtime output.
/// Media detection is scoped to URL-like contexts to avoid false positives on
/// filenames mentioned in prose.
fn section_has_substantive_evidence(section: &str) -> bool {
    let lower = section.to_lowercase();
    let media_exts = [".png", ".jpg", ".jpeg", ".gif", ".mp4", ".mov", ".webm"];

    // Check for markdown images with non-empty URLs: ![...](http...)
    for line in lower.lines() {
        if let Some(start) = line.find("![") {
            let rest = &line[start..];
            if let Some(paren) = rest.find("](") {
                let after = &rest[paren + 2..];
                if after.starts_with("http") {
                    return true;
                }
            }
        }
    }

    // Check for HTML <img src="http..."> tags.
    for url in html_img_src_urls(section) {
        if url.starts_with("http") {
            return true;
        }
    }

    // Check for media extension in URL-like words (http...)
    for line in lower.lines() {
        for word in line.split_whitespace() {
            if word.starts_with("http") && media_exts.iter().any(|ext| word.contains(ext)) {
                return true;
            }
        }
    }

    // Code blocks under evidence headings indicate terminal output.
    if section.contains("```") {
        return true;
    }

    false
}

/// Classify evidence status for captain review context.
pub(crate) fn evidence_status(ctx: &WorkerContext) -> &'static str {
    if ctx.pr_body.is_empty() {
        return "unknown (no PR body)";
    }
    if !has_no_evidence(&ctx.pr_body) {
        if !evidence_is_fresh(ctx) {
            return "STALE (evidence exists but predates reopen)";
        }
        return "needs-review (has evidence text, quality unknown)";
    }
    "MISSING"
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
        }
    }

    #[test]
    fn empty_body_is_inconclusive() {
        // Empty body = fetch failure or no PR, not "missing evidence".
        assert!(!has_no_evidence(""));
    }

    #[test]
    fn evidence_present_after_heading_with_image() {
        assert!(!has_no_evidence(
            "## Summary\n### After\n![fix](https://example.com/fix.png)"
        ));
    }

    #[test]
    fn evidence_present_visual_with_image() {
        assert!(!has_no_evidence(
            "## Visual Evidence\n![screenshot](https://example.com/shot.png)"
        ));
    }

    #[test]
    fn evidence_present_github_attachment_markdown_without_extension() {
        assert!(!has_no_evidence(
            "## Evidence\n![fix](https://github.com/user-attachments/assets/1234abcd)"
        ));
    }

    #[test]
    fn evidence_present_html_image_without_extension() {
        assert!(!has_no_evidence(
            r#"## Evidence
<img src="https://github.com/user-attachments/assets/abcd-1234" alt="proof" />"#
        ));
    }

    #[test]
    fn evidence_present_uppercase_html_image() {
        assert!(!has_no_evidence(
            r#"## Evidence
<IMG SRC="https://github.com/user-attachments/assets/abcd-1234" alt="proof" />"#
        ));
    }

    #[test]
    fn evidence_heading_only_no_content() {
        assert!(has_no_evidence(
            "## Summary\n### After\nJust some text, no images or code blocks"
        ));
    }

    #[test]
    fn ignores_badges_outside_evidence_section() {
        assert!(has_no_evidence(
            r#"## Evidence
Just some text, no screenshots yet.

## Footer
<img src="https://example.com/badge.png" alt="badge" />"#
        ));
    }

    #[test]
    fn evidence_heading_with_code_block() {
        assert!(!has_no_evidence(
            "### After\n```\n$ cargo nextest run -p mando-types\n     Summary [ 0.042s] 27 tests run: 27 passed, 0 skipped\n```"
        ));
    }

    #[test]
    fn summary_diagram_missing() {
        let ctx = make_ctx();
        assert!(!has_summary_diagram(&ctx));
    }

    #[test]
    fn summary_diagram_present() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## PR Summary\n```mermaid\n```".into();
        ctx.reopen_seq = 0;
        assert!(has_summary_diagram(&ctx));
    }

    #[test]
    fn summary_diagram_stale_after_reopen() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## PR Summary\n<!-- pr-summary-head: def456 -->\n```mermaid\n```".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123".into();
        assert!(!has_summary_diagram(&ctx));
    }

    #[test]
    fn summary_diagram_fresh_after_reopen() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## PR Summary\n<!-- pr-summary-head: abc123 -->\n```mermaid\n```".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123def".into();
        assert!(has_summary_diagram(&ctx));
    }

    #[test]
    fn evidence_fresh_no_reopen() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## Evidence\n![fix](https://example.com/fix.png)".into();
        ctx.reopen_seq = 0;
        assert!(evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_stale_after_reopen_no_marker() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## Evidence\n![fix](https://example.com/fix.png)".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123".into();
        assert!(!evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_stale_after_reopen_wrong_sha() {
        let mut ctx = make_ctx();
        ctx.pr_body =
            "## Evidence\n![fix](https://example.com/fix.png)\n<!-- evidence-head: def456 -->"
                .into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123".into();
        assert!(!evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_fresh_after_reopen_matching_sha() {
        let mut ctx = make_ctx();
        ctx.pr_body =
            "## Evidence\n![fix](https://example.com/fix.png)\n<!-- evidence-head: abc123 -->"
                .into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123def".into();
        assert!(evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_stale_when_marker_sha_empty() {
        let mut ctx = make_ctx();
        ctx.pr_body =
            "## Evidence\n![fix](https://example.com/fix.png)\n<!-- evidence-head:  -->".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123".into();
        assert!(!evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_fresh_when_head_sha_empty() {
        let mut ctx = make_ctx();
        ctx.pr_body = "## Evidence\n![fix](https://example.com/fix.png)".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = String::new();
        assert!(evidence_is_fresh(&ctx));
    }

    #[test]
    fn evidence_status_missing() {
        let mut ctx = make_ctx();
        ctx.pr_body = "Just a description".into();
        assert_eq!(evidence_status(&ctx), "MISSING");
    }

    #[test]
    fn evidence_status_needs_review() {
        let mut ctx = make_ctx();
        ctx.pr_body = "### After\n![fix](https://example.com/fix.png)".into();
        assert_eq!(
            evidence_status(&ctx),
            "needs-review (has evidence text, quality unknown)"
        );
    }

    #[test]
    fn evidence_status_stale_after_reopen() {
        let mut ctx = make_ctx();
        ctx.pr_body = "### After\n![fix](https://example.com/fix.png)".into();
        ctx.reopen_seq = 1;
        ctx.pr_head_sha = "abc123".into();
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
