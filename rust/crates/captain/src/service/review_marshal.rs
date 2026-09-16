//! Review marshal. Parses PR reviews and classifies reviewer verdicts.

// ── PR Hygiene ───────────────────────────────────────────────────────────────

/// Known bot login suffixes and exact matches.
const BOT_LOGINS: &[&str] = &["[bot]", "github-actions", "dependabot"];

/// Patterns for noise comments we should ignore (triggers, linkbacks).
const NOISE_PATTERNS: &[&str] = &[
    "<!--",
    "@codex review",
    "cursor review",
    "bugbot run",
    "linear-linkback",
];

/// Patterns that indicate a bot review (valuable, not noise).
const BOT_REVIEW_PATTERNS: &[&str] = &["Codex Review", "Devin Review"];

fn is_bot_login(login: &str) -> bool {
    BOT_LOGINS
        .iter()
        .any(|pat| login.ends_with(pat) || login == *pat)
}

fn is_noise_comment(body: &str) -> bool {
    let lower = body.to_lowercase();
    NOISE_PATTERNS.iter().any(|pat| lower.contains(pat))
}

fn is_bot_review(body: &str) -> bool {
    BOT_REVIEW_PATTERNS.iter().any(|pat| body.contains(pat))
}

/// Terminal phrases indicating a bot review found no actionable issues.
/// Only unambiguous signals — avoid preamble phrases like "no major issues"
/// that often precede actual findings.
const CLEAN_REVIEW_PATTERNS: &[&str] = &[
    "no issues found",
    "no new issues",
    "no problems found",
    "found no issues",
    "found no problems",
    "didn't find any",
    "did not find any",
    "nothing to flag",
    "nothing to report",
    "no action needed",
    "no action required",
];

/// A bot review that contains no actionable findings — just "LGTM" / "no issues".
fn is_clean_bot_review(body: &str) -> bool {
    if !is_bot_review(body) {
        return false;
    }
    let lower = body.to_lowercase();
    // Check for explicit clean signals.
    if CLEAN_REVIEW_PATTERNS.iter().any(|pat| lower.contains(pat)) {
        tracing::debug!(
            body_len = body.len(),
            "skipping clean bot review (pattern match)"
        );
        return true;
    }
    // A short bot review that's just "LGTM" or similar.
    if lower.contains("lgtm") && body.len() < 500 {
        tracing::debug!(
            body_len = body.len(),
            "skipping clean bot review (short LGTM)"
        );
        return true;
    }
    false
}

/// Count unaddressed issue-level comments on a PR.
///
/// Uses a watermark approach: the most recent `[Mando]`-prefixed comment
/// by the PR author marks all earlier comments as addressed. Returns the
/// count of non-author, non-noise comments after the watermark.
pub(crate) fn issue_comment_hygiene(comments: &[global_github::PrComment], pr_author: &str) -> i64 {
    let author_norm = pr_author.to_lowercase();

    // Find watermark: most recent [Mando] ack by PR author.
    let watermark_ts = comments
        .iter()
        .rev()
        .find(|c| c.user.to_lowercase() == author_norm && c.body.starts_with("[Mando]"))
        .map(|c| c.created_at.as_str())
        .unwrap_or("");

    let mut unaddressed = 0i64;
    for comment in comments {
        let login = comment.user.to_lowercase();

        // Skip PR author's own comments.
        if login == author_norm {
            continue;
        }
        // Skip empty comments.
        if comment.body.trim().is_empty() {
            continue;
        }
        // Skip comments before watermark.
        if !watermark_ts.is_empty() && comment.created_at.as_str() <= watermark_ts {
            continue;
        }

        let is_bot = is_bot_login(&comment.user);

        // Skip noise comments unless they're bot reviews.
        if is_noise_comment(&comment.body) && !is_bot_review(&comment.body) {
            continue;
        }
        // Skip non-review bot comments.
        if is_bot && !is_bot_review(&comment.body) {
            continue;
        }
        // Skip bot reviews that found no actionable issues (LGTM / "no issues").
        if is_bot && is_clean_bot_review(&comment.body) {
            continue;
        }

        unaddressed += 1;
    }

    unaddressed
}

/// Analyze review threads for hygiene.
///
/// Returns `(unresolved, unreplied)` counts from the structured thread data.
/// - unresolved: thread not resolved AND PR author hasn't replied
/// - unreplied: subset — reviewer commented but author hasn't replied
pub(crate) fn thread_hygiene(
    threads: &[global_github::ReviewThread],
    pr_author: &str,
) -> (i64, i64) {
    let author_norm = pr_author.to_lowercase();
    let mut unresolved = 0i64;
    let mut unreplied = 0i64;

    for thread in threads {
        if thread.is_resolved {
            continue;
        }

        let mut has_author_reply = false;
        let mut has_reviewer_comment = false;

        for comment in &thread.comments {
            if comment.author.to_lowercase() == author_norm {
                has_author_reply = true;
            } else {
                has_reviewer_comment = true;
            }
        }

        if !has_author_reply {
            unresolved += 1;
            if has_reviewer_comment {
                unreplied += 1;
            }
        }
    }

    (unresolved, unreplied)
}
