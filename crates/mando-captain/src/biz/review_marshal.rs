//! Review marshal. Parses PR reviews and classifies reviewer verdicts.

#[cfg(test)]
use serde::{Deserialize, Serialize};

/// Per-reviewer verdict.
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewerVerdict {
    pub reviewer: String,
    pub status: ReviewStatus,
    pub detail: String,
}

/// Possible review outcomes.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewStatus {
    Approved,
    Blocked,
    Pending,
    ChangesRequested,
}

/// Aggregate review gate result.
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewGateResult {
    pub state: ReviewGateState,
    pub summary: String,
    pub reviewer_verdicts: Vec<ReviewerVerdict>,
    pub unresolved_threads: usize,
    pub unreplied_threads: usize,
}

/// Overall gate state.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewGateState {
    Pass,
    Fail,
    Pending,
    Unknown,
}

/// Parse PR review data from `gh pr view` JSON output.
#[cfg(test)]
fn evaluate_reviews(pr_json: &serde_json::Value) -> ReviewGateResult {
    let empty_reviews = vec![];
    let reviews = pr_json["reviews"].as_array().unwrap_or(&empty_reviews);

    let mut verdicts: Vec<ReviewerVerdict> = Vec::new();
    let mut seen_reviewers = std::collections::HashSet::new();

    // Process reviews in reverse chronological order (latest first).
    for review in reviews.iter().rev() {
        let reviewer = review["author"]["login"].as_str().unwrap_or("").to_string();
        if reviewer.is_empty() || seen_reviewers.contains(&reviewer) {
            continue;
        }
        seen_reviewers.insert(reviewer.clone());

        let state = review["state"].as_str().unwrap_or("");
        let body = review["body"].as_str().unwrap_or("").to_string();

        let status = match state {
            "APPROVED" => ReviewStatus::Approved,
            "CHANGES_REQUESTED" => ReviewStatus::ChangesRequested,
            "DISMISSED" => continue,
            "COMMENTED" => ReviewStatus::Pending,
            _ => ReviewStatus::Pending,
        };

        verdicts.push(ReviewerVerdict {
            reviewer,
            status,
            detail: body,
        });
    }

    // Count unresolved review threads.
    let empty_threads = vec![];
    let review_threads = pr_json["reviewThreads"]["nodes"]
        .as_array()
        .unwrap_or(&empty_threads);
    let unresolved_threads = review_threads
        .iter()
        .filter(|t| !t["isResolved"].as_bool().unwrap_or(true))
        .count();

    // Count unreplied threads (last comment is not by PR author).
    let pr_author = pr_json["author"]["login"].as_str().unwrap_or("");
    let unreplied_threads = review_threads
        .iter()
        .filter(|t| {
            let comments = t["comments"]["nodes"].as_array();
            if let Some(comments) = comments {
                if let Some(last) = comments.last() {
                    let commenter = last["author"]["login"].as_str().unwrap_or("");
                    return commenter != pr_author && !t["isResolved"].as_bool().unwrap_or(true);
                }
            }
            false
        })
        .count();

    // Determine gate state.
    let has_blocking = verdicts
        .iter()
        .any(|v| v.status == ReviewStatus::ChangesRequested || v.status == ReviewStatus::Blocked);
    let all_approved =
        !verdicts.is_empty() && verdicts.iter().all(|v| v.status == ReviewStatus::Approved);

    let state = if has_blocking {
        ReviewGateState::Fail
    } else if all_approved && unresolved_threads == 0 {
        ReviewGateState::Pass
    } else if verdicts.is_empty() {
        ReviewGateState::Unknown
    } else {
        ReviewGateState::Pending
    };

    let summary = match state {
        ReviewGateState::Pass => "All reviewers approved, no unresolved threads".into(),
        ReviewGateState::Fail => format!(
            "Blocked by {} reviewer(s), {} unresolved threads",
            verdicts
                .iter()
                .filter(|v| v.status == ReviewStatus::ChangesRequested)
                .count(),
            unresolved_threads
        ),
        ReviewGateState::Pending => format!(
            "{} reviewer(s) pending, {} unresolved threads",
            verdicts
                .iter()
                .filter(|v| v.status == ReviewStatus::Pending)
                .count(),
            unresolved_threads
        ),
        ReviewGateState::Unknown => "No reviews yet".into(),
    };

    ReviewGateResult {
        state,
        summary,
        reviewer_verdicts: verdicts,
        unresolved_threads,
        unreplied_threads,
    }
}

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
pub(crate) fn issue_comment_hygiene(
    comments: &[crate::io::github_pr::PrComment],
    pr_author: &str,
) -> i64 {
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
    threads: &[crate::io::github_pr::ReviewThread],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reviews() {
        let pr = serde_json::json!({"reviews": []});
        let result = evaluate_reviews(&pr);
        assert_eq!(result.state, ReviewGateState::Unknown);
    }

    #[test]
    fn all_approved() {
        let pr = serde_json::json!({
            "reviews": [
                {"author": {"login": "alice"}, "state": "APPROVED", "body": "LGTM"},
            ],
            "reviewThreads": {"nodes": []},
        });
        let result = evaluate_reviews(&pr);
        assert_eq!(result.state, ReviewGateState::Pass);
    }

    #[test]
    fn changes_requested() {
        let pr = serde_json::json!({
            "reviews": [
                {"author": {"login": "bob"}, "state": "CHANGES_REQUESTED", "body": "fix this"},
            ],
            "reviewThreads": {"nodes": []},
        });
        let result = evaluate_reviews(&pr);
        assert_eq!(result.state, ReviewGateState::Fail);
        assert_eq!(
            result.reviewer_verdicts[0].status,
            ReviewStatus::ChangesRequested
        );
    }

    #[test]
    fn latest_review_wins() {
        let pr = serde_json::json!({
            "reviews": [
                {"author": {"login": "alice"}, "state": "CHANGES_REQUESTED", "body": "fix"},
                {"author": {"login": "alice"}, "state": "APPROVED", "body": "ok now"},
            ],
            "reviewThreads": {"nodes": []},
        });
        let result = evaluate_reviews(&pr);
        assert_eq!(result.state, ReviewGateState::Pass);
    }

    // ── issue_comment_hygiene tests ──

    use crate::io::github_pr::{PrComment, ReviewThread, ThreadComment};

    fn make_comment(author: &str, body: &str, created_at: &str) -> PrComment {
        PrComment {
            id: 0,
            user: author.into(),
            body: body.into(),
            created_at: created_at.into(),
        }
    }

    #[test]
    fn issue_hygiene_counts_reviewer_comments() {
        let comments = vec![
            make_comment("reviewer1", "Please fix this", "2024-01-01T00:00:00Z"),
            make_comment("reviewer2", "Also this", "2024-01-02T00:00:00Z"),
        ];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 2);
    }

    #[test]
    fn issue_hygiene_skips_author_comments() {
        let comments = vec![
            make_comment("author", "I fixed it", "2024-01-01T00:00:00Z"),
            make_comment("reviewer1", "Not yet", "2024-01-02T00:00:00Z"),
        ];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 1);
    }

    #[test]
    fn issue_hygiene_watermark_clears_old() {
        let comments = vec![
            make_comment("reviewer1", "Fix this", "2024-01-01T00:00:00Z"),
            make_comment(
                "author",
                "[Mando] Addressed all feedback",
                "2024-01-02T00:00:00Z",
            ),
            make_comment("reviewer1", "New issue", "2024-01-03T00:00:00Z"),
        ];
        // Only the comment after watermark counts.
        assert_eq!(issue_comment_hygiene(&comments, "author"), 1);
    }

    #[test]
    fn issue_hygiene_skips_noise() {
        let comments = vec![make_comment(
            "bot[bot]",
            "<!-- linear-linkback -->",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 0);
    }

    #[test]
    fn issue_hygiene_skips_clean_bot_reviews() {
        // Bot review that found nothing — should NOT count as unaddressed.
        let comments = vec![make_comment(
            "codex[bot]",
            "Codex Review\nLGTM",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 0);
    }

    #[test]
    fn issue_hygiene_skips_clean_bot_review_no_issues() {
        let comments = vec![make_comment(
            "devin-ai-integration[bot]",
            "Devin Review\n\nNo issues found. The code looks correct.",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 0);
    }

    #[test]
    fn issue_hygiene_skips_clean_bot_review_didnt_find_any() {
        // "Didn't find any" is conclusive — the bot searched and found nothing.
        let comments = vec![make_comment(
            "chatgpt-codex-connector[bot]",
            "Codex Review: Didn't find any major issues. :tada:",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 0);
    }

    #[test]
    fn issue_hygiene_keeps_ambiguous_bot_reviews() {
        // "no major issues" is ambiguous — could precede minor findings.
        let comments = vec![make_comment(
            "codex[bot]",
            "Codex Review\n\nNo major issues, but here are 3 minor bugs:\n1. Missing null check\n2. Unused var\n3. Wrong return type",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 1);
    }

    #[test]
    fn issue_hygiene_lgtm_length_boundary() {
        // Short LGTM review (< 500 chars) → clean.
        let short = make_comment("codex[bot]", "Codex Review\nLGTM", "2024-01-01T00:00:00Z");
        assert_eq!(issue_comment_hygiene(&[short], "author"), 0);

        // Long review containing "LGTM" (>= 500 chars) → NOT clean.
        let long_body = format!(
            "Codex Review\nLGTM but here are details:\n{}",
            "x".repeat(500)
        );
        let long = make_comment("codex[bot]", &long_body, "2024-01-01T00:00:00Z");
        assert_eq!(issue_comment_hygiene(&[long], "author"), 1);
    }

    #[test]
    fn issue_hygiene_keeps_actionable_bot_reviews() {
        // Bot review with real findings — SHOULD count as unaddressed.
        let comments = vec![make_comment(
            "codex[bot]",
            "Codex Review\n\nFound 2 issues:\n1. Missing error handling in parse()\n2. Unused import on line 5",
            "2024-01-01T00:00:00Z",
        )];
        assert_eq!(issue_comment_hygiene(&comments, "author"), 1);
    }

    // ── thread_hygiene tests ──

    fn make_thread(resolved: bool, comments: Vec<(&str, &str)>) -> ReviewThread {
        ReviewThread {
            id: "t1".into(),
            is_resolved: resolved,
            comments: comments
                .into_iter()
                .map(|(author, body)| ThreadComment {
                    author: author.into(),
                    body: body.into(),
                    path: None,
                    line: None,
                })
                .collect(),
        }
    }

    #[test]
    fn thread_hygiene_resolved_ignored() {
        let threads = vec![make_thread(true, vec![("reviewer", "fix this")])];
        let (unresolved, unreplied) = thread_hygiene(&threads, "author");
        assert_eq!(unresolved, 0);
        assert_eq!(unreplied, 0);
    }

    #[test]
    fn thread_hygiene_unresolved_no_author_reply() {
        let threads = vec![make_thread(false, vec![("reviewer", "fix this")])];
        let (unresolved, unreplied) = thread_hygiene(&threads, "author");
        assert_eq!(unresolved, 1);
        assert_eq!(unreplied, 1);
    }

    #[test]
    fn thread_hygiene_author_replied() {
        let threads = vec![make_thread(
            false,
            vec![("reviewer", "fix this"), ("author", "done")],
        )];
        let (unresolved, unreplied) = thread_hygiene(&threads, "author");
        assert_eq!(unresolved, 0);
        assert_eq!(unreplied, 0);
    }
}
