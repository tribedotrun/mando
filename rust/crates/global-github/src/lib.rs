//! GitHub CLI/API provider boundary.
//!
//! This crate is the only production code allowed to spawn `gh`. Callers own
//! orchestration policy; this crate owns command execution, retries, upstream
//! JSON parsing, and typed GitHub response shapes.

mod command;
mod review_threads;
mod types;

use anyhow::{Context, Result};
use command::{run_gh, run_gh_api_paginate, run_gh_bytes, run_gh_capture, run_gh_in_dir};
use serde::Deserialize;
use std::path::Path;

pub use review_threads::get_pr_review_threads;
pub use types::{
    MergeBlockReason, MergeOutcome, MergeableStatus, PrComment, PrState, PrStatus, ReviewDecision,
    ReviewThread, ThreadComment,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubUserAttachment {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

fn valid_user_attachment_id(asset_id: &str) -> bool {
    asset_id.len() == 36
        && asset_id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn attachment_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"\x1a\x45\xdf\xa3") {
        "video/webm"
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        if &bytes[8..12] == b"qt  " {
            "video/quicktime"
        } else {
            "video/mp4"
        }
    } else {
        "application/octet-stream"
    }
}

/// Download an authenticated GitHub user attachment through the sole `gh`
/// command boundary. Private-repository attachment URLs otherwise return 404
/// when loaded directly by Electron.
pub async fn get_user_attachment(asset_id: &str) -> Result<GitHubUserAttachment> {
    if !valid_user_attachment_id(asset_id) {
        anyhow::bail!("invalid GitHub user attachment id");
    }

    let url = format!("https://github.com/user-attachments/assets/{asset_id}");
    let bytes = run_gh_bytes(&["api", "-H", "Accept: application/octet-stream", &url]).await?;
    let content_type = attachment_content_type(&bytes);
    Ok(GitHubUserAttachment {
        bytes,
        content_type,
    })
}

#[derive(Debug, Deserialize)]
struct GhAuthor {
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhStatusCheck {
    conclusion: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrFile {
    path: String,
}

#[derive(Debug, Deserialize)]
struct GhPrViewResponse {
    author: Option<GhAuthor>,
    body: Option<String>,
    #[serde(rename = "headRefOid")]
    head_ref_oid: Option<String>,
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<Vec<GhStatusCheck>>,
    comments: Option<Vec<serde_json::Value>>,
    files: Option<Vec<GhPrFile>>,
}

#[derive(Debug, Deserialize)]
struct GhPrCommitsResponse {
    commits: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct GhPrListEntry {
    url: String,
}

#[derive(Debug, Deserialize)]
struct GhPrStateResponse {
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrMergeableResponse {
    state: Option<String>,
    mergeable: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrHeadResponse {
    #[serde(rename = "headRefOid")]
    head_ref_oid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrReviewDecisionResponse {
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
}

pub async fn fetch_pr_status(repo: &str, pr_number: &str) -> Result<PrStatus> {
    let text = run_gh(&[
        "pr",
        "view",
        pr_number,
        "--repo",
        repo,
        "--json",
        "number,author,body,headRefOid,isDraft,statusCheckRollup,comments,files",
    ])
    .await?;
    let parsed: GhPrViewResponse = serde_json::from_str(&text).context("parse gh pr view JSON")?;

    let author = parsed.author.and_then(|a| a.login).unwrap_or_default();
    let body = parsed.body.unwrap_or_default();
    let head_sha = parsed.head_ref_oid.unwrap_or_default();
    let is_draft = parsed.is_draft.unwrap_or(false);

    let ci_status = parsed.status_check_rollup.map(|arr| {
        let is_failure = |c: &GhStatusCheck| -> bool {
            let s = c
                .conclusion
                .as_deref()
                .or(c.state.as_deref())
                .unwrap_or("PENDING");
            matches!(
                s,
                "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
            )
        };
        let is_success = |c: &GhStatusCheck| -> bool {
            let s = c
                .conclusion
                .as_deref()
                .or(c.state.as_deref())
                .unwrap_or("PENDING");
            matches!(s, "SUCCESS" | "SKIPPED" | "NEUTRAL")
        };

        if arr.iter().any(is_failure) {
            "failure".to_string()
        } else if arr.iter().all(is_success) {
            "success".to_string()
        } else {
            "pending".to_string()
        }
    });

    let comments = parsed.comments.map(|a| a.len() as i64).unwrap_or(0);
    let changed_files = parsed
        .files
        .map(|arr| arr.into_iter().map(|f| f.path).collect())
        .unwrap_or_default();

    Ok(PrStatus {
        author,
        ci_status,
        comments,
        unresolved_threads: 0,
        unreplied_threads: 0,
        unaddressed_issue_comments: 0,
        body,
        head_sha,
        is_draft,
        changed_files,
    })
}

pub async fn is_pr_merged(repo: &str, pr_number: &str) -> Result<bool> {
    Ok(matches!(pr_state(repo, pr_number).await?, PrState::Merged))
}

pub async fn is_pr_branch_ahead(repo: &str, pr_number: &str) -> Result<bool> {
    let text = run_gh(&["pr", "view", pr_number, "--repo", repo, "--json", "commits"]).await?;
    let parsed: GhPrCommitsResponse =
        serde_json::from_str(&text).context("parse gh pr view commits JSON")?;
    let commits = parsed.commits.as_deref().map(|a| a.len()).unwrap_or(0);
    Ok(commits > 0)
}

pub async fn close_pr(repo: &str, pr_number: &str) -> Result<()> {
    run_gh(&["pr", "close", pr_number, "--repo", repo]).await?;
    Ok(())
}

pub async fn discover_pr_for_branch(repo: &str, branch: &str) -> Option<i64> {
    let text = match run_gh(&[
        "pr", "list", "--repo", repo, "--head", branch, "--state", "open", "--json", "url",
        "--limit", "1",
    ])
    .await
    {
        Ok(text) => text,
        Err(e) => {
            tracing::warn!(module = "github", repo = %repo, branch = %branch, error = %e, "gh pr list failed");
            return None;
        }
    };

    let arr: Vec<GhPrListEntry> = match serde_json::from_str(&text) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(module = "github", repo = %repo, branch = %branch, error = %e, "failed to parse gh pr list JSON");
            return None;
        }
    };
    arr.first().and_then(|v| parse_pr_number(&v.url))
}

pub async fn pr_state(repo: &str, pr_number: &str) -> Result<PrState> {
    let text = run_gh(&["pr", "view", pr_number, "--repo", repo, "--json", "state"]).await?;
    let parsed: GhPrStateResponse =
        serde_json::from_str(&text).context("parse gh pr state JSON")?;
    Ok(match parsed.state.as_deref() {
        Some("OPEN") => PrState::Open,
        Some("CLOSED") => PrState::Closed,
        Some("MERGED") => PrState::Merged,
        Some(other) => PrState::Unknown(other.to_string()),
        None => PrState::Unknown(String::new()),
    })
}

pub async fn check_pr_mergeable(pr: &str, repo: &str) -> Result<MergeableStatus> {
    let pr_num = pr.trim_start_matches('#');
    let mut args = vec![
        "pr",
        "view",
        pr_num,
        "--json",
        "state,mergeable,mergeStateStatus",
    ];
    if !repo.is_empty() {
        args.extend(["--repo", repo]);
    }
    let text = run_gh(&args).await?;
    let parsed: GhPrMergeableResponse =
        serde_json::from_str(&text).context("parse gh pr mergeability JSON")?;
    let state = parsed.state.as_deref().ok_or_else(|| {
        anyhow::anyhow!("gh pr view response missing `state` field for PR {pr} in {repo}")
    })?;
    let mergeable = parsed.mergeable.as_deref().ok_or_else(|| {
        anyhow::anyhow!("gh pr view response missing `mergeable` field for PR {pr} in {repo}")
    })?;

    match state {
        "MERGED" => Ok(MergeableStatus::Merged),
        "CLOSED" => Ok(MergeableStatus::Closed),
        _ => match mergeable {
            "MERGEABLE" => Ok(MergeableStatus::Mergeable),
            "CONFLICTING" => Ok(MergeableStatus::Conflicted),
            _ => Ok(MergeableStatus::Unknown),
        },
    }
}

/// Squash-merge a pull request deterministically, without an agent session.
///
/// Runs `gh pr merge <pr_number> --repo <repo> --squash --subject <subject>`,
/// appending `--admin` only when the caller asks for it — this crate never
/// adds an unconditional administrator override. A refused merge returns
/// [`MergeOutcome::Blocked`] with a classified [`MergeBlockReason`] plus gh's
/// verbatim output; `Err` means gh itself could not be run.
///
/// `subject` is passed explicitly because GitHub only derives the
/// `<title> (#<pr>)` squash subject for a multi-commit PR; a single-commit PR
/// otherwise lands under the lone commit's own subject, dropping the PR
/// reference the repo's history convention depends on.
///
/// Retry policy (including whether an `--admin` second attempt is warranted)
/// belongs to the caller. See [`MergeBlockReason::admin_can_bypass`] for the
/// capability hint and [`pr_review_decision`] for the check that must precede
/// any override, and note that gh reports a merge of an already-merged PR
/// as a failure — confirm with [`is_pr_merged`] before treating
/// [`MergeBlockReason::Other`] as a real block.
pub async fn merge_pr_squash(
    repo: &str,
    pr_number: &str,
    subject: &str,
    admin: bool,
) -> Result<MergeOutcome> {
    let mut args = vec![
        "pr",
        "merge",
        pr_number,
        "--repo",
        repo,
        "--squash",
        "--subject",
        subject,
    ];
    if admin {
        args.push("--admin");
    }
    let output = run_gh_capture(&args).await.context("gh pr merge")?;

    if output.success {
        tracing::info!(module = "github", repo = %repo, pr = %pr_number, admin = admin, "squash-merged PR");
        return Ok(MergeOutcome::Merged);
    }

    let detail = merge_block_detail(&output.stdout, &output.stderr);
    let reason = classify_merge_block(&detail);
    tracing::warn!(module = "github", repo = %repo, pr = %pr_number, admin = admin, reason = ?reason, detail = %detail, "squash-merge blocked");
    Ok(MergeOutcome::Blocked { reason, detail })
}

/// Ask GitHub whether a human review still stands between this PR and a
/// merge, via `gh pr view --json reviewDecision`.
///
/// This is the machine-readable answer [`classify_merge_block`] cannot give:
/// gh's client-side precheck reports every branch-protection rule — required
/// approvals included — through one umbrella "base branch policy prohibits
/// the merge" message. Any caller about to retry with `--admin` must check
/// [`ReviewDecision::blocks_admin_merge`] first.
///
/// An unrecognized value is [`ReviewDecision::ReviewRequired`], the
/// conservative reading: an unknown decision is not proof that nothing is
/// outstanding.
pub async fn pr_review_decision(repo: &str, pr_number: &str) -> Result<ReviewDecision> {
    let text = run_gh(&[
        "pr",
        "view",
        pr_number,
        "--repo",
        repo,
        "--json",
        "reviewDecision",
    ])
    .await?;
    let parsed: GhPrReviewDecisionResponse =
        serde_json::from_str(&text).context("parse gh pr reviewDecision JSON")?;
    Ok(classify_review_decision(parsed.review_decision.as_deref()))
}

/// GitHub returns an empty string (or omits the field) when the repository
/// requires no review on this PR.
fn classify_review_decision(raw: Option<&str>) -> ReviewDecision {
    match raw.map(str::trim).unwrap_or("") {
        "" | "null" => ReviewDecision::NotRequired,
        "APPROVED" => ReviewDecision::Approved,
        "CHANGES_REQUESTED" => ReviewDecision::ChangesRequested,
        other => {
            if other != "REVIEW_REQUIRED" {
                tracing::warn!(
                    module = "github",
                    review_decision = %other,
                    "unrecognized gh reviewDecision; treating as review required"
                );
            }
            ReviewDecision::ReviewRequired
        }
    }
}

/// Phrases naming a missing human approving review. Checked first so a block
/// that mentions review approval is never reported as administrator-clearable.
const REVIEW_REQUIRED_PATTERNS: &[&str] = &[
    "approving review",
    "review is required",
    "reviews are required",
    "changes requested",
];

/// Phrases naming branch protection or required status checks.
const BRANCH_PROTECTION_PATTERNS: &[&str] = &[
    "protected branch rules",
    "required status check",
    "base branch policy",
    "changes must be made through a pull request",
];

/// Phrases naming a pull request that cannot be merged as it stands.
const NOT_MERGEABLE_PATTERNS: &[&str] = &[
    "not mergeable",
    "merge conflict",
    "cannot be cleanly created",
];

/// Combine gh's streams into the text shown to humans and fed to the
/// classifier. gh writes its refusal to stderr, but keep stdout so a future
/// message shape is not silently dropped.
fn merge_block_detail(stdout: &str, stderr: &str) -> String {
    let parts: Vec<&str> = [stderr.trim(), stdout.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return "gh pr merge failed with no output".to_string();
    }
    parts.join("\n")
}

/// Classify gh's refusal text. gh exposes no machine-readable failure code for
/// `pr merge`, so its own wording is the only signal; anything unrecognized is
/// [`MergeBlockReason::Other`] rather than a guess.
fn classify_merge_block(detail: &str) -> MergeBlockReason {
    let lower = detail.to_lowercase();
    let matches_any = |patterns: &[&str]| patterns.iter().any(|p| lower.contains(p));

    if matches_any(REVIEW_REQUIRED_PATTERNS) {
        MergeBlockReason::ReviewRequired
    } else if matches_any(BRANCH_PROTECTION_PATTERNS) {
        MergeBlockReason::BranchProtection
    } else if matches_any(NOT_MERGEABLE_PATTERNS) {
        MergeBlockReason::NotMergeable
    } else {
        MergeBlockReason::Other
    }
}

pub async fn current_pr_head_sha(repo: &str, pr_num: i64) -> Result<String> {
    let text = run_gh(&[
        "pr",
        "view",
        &pr_num.to_string(),
        "--repo",
        repo,
        "--json",
        "headRefOid",
    ])
    .await?;
    let parsed: GhPrHeadResponse = serde_json::from_str(&text).context("parse gh pr head JSON")?;
    let sha = parsed
        .head_ref_oid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("gh pr view response missing headRefOid"))?;
    if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!("gh pr view returned invalid headRefOid"));
    }
    Ok(sha.to_string())
}

pub async fn create_draft_pr(cwd: &Path, title: &str, body: &str) -> Result<i64> {
    let url = run_gh_in_dir(
        cwd,
        &["pr", "create", "--draft", "--title", title, "--body", body],
    )
    .await
    .context("gh pr create")?;
    parse_pr_number(url.trim()).context("failed to parse PR number from gh output")
}

pub async fn get_pr_body(repo: &str, pr: u32) -> Result<String> {
    let endpoint = format!("repos/{repo}/pulls/{pr}");
    let body = run_gh(&["api", &endpoint, "--jq", ".body"]).await?;
    let trimmed = body.trim();
    if trimmed == "null" || trimmed.is_empty() {
        return Ok(String::new());
    }
    Ok(trimmed.to_string())
}

pub async fn get_pr_comments(repo: &str, pr: u32) -> Result<Vec<PrComment>> {
    let endpoint = format!("repos/{repo}/issues/{pr}/comments");
    let items = run_gh_api_paginate(&[&endpoint]).await?;
    let total = items.len();
    let comments: Vec<PrComment> = items
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<PrComment>(v) {
            Ok(c) if !c.user.is_empty() => Some(c),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(module = "global-github", pr = pr, error = %e, "skipping unparseable PR comment");
                None
            }
        })
        .collect();
    if comments.is_empty() && total > 0 {
        return Err(anyhow::anyhow!(
            "all {total} PR comments failed to parse for pr #{pr} in {repo}, possible API schema drift"
        ));
    }
    Ok(comments)
}

fn parse_pr_number(url: &str) -> Option<i64> {
    url.rsplit('/').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        attachment_content_type, classify_merge_block, classify_review_decision,
        merge_block_detail, valid_user_attachment_id, MergeBlockReason, ReviewDecision,
    };

    #[test]
    fn validates_only_uuid_shaped_user_attachment_ids() {
        assert!(valid_user_attachment_id(
            "196ce199-c4c7-4761-8779-a77e02234ae5"
        ));
        assert!(!valid_user_attachment_id("../../settings"));
        assert!(!valid_user_attachment_id(
            "196ce199-c4c7-4761-8779-a77e02234aeZ"
        ));
    }

    #[test]
    fn detects_pr_image_and_recording_content_types() {
        assert_eq!(
            attachment_content_type(b"\x89PNG\r\n\x1a\nrest"),
            "image/png"
        );
        assert_eq!(
            attachment_content_type(b"\0\0\0\x18ftypisomrest"),
            "video/mp4"
        );
        assert_eq!(
            attachment_content_type(b"\0\0\0\x18ftypqt  rest"),
            "video/quicktime"
        );
        assert_eq!(
            attachment_content_type(b"unknown"),
            "application/octet-stream"
        );
    }

    #[test]
    fn classifies_missing_approving_review() {
        assert_eq!(
            classify_merge_block(
                "GraphQL: At least 1 approving review is required by reviewers with write access. (mergePullRequest)"
            ),
            MergeBlockReason::ReviewRequired
        );
    }

    #[test]
    fn classifies_multiple_required_reviews() {
        assert_eq!(
            classify_merge_block(
                "GraphQL: At least 2 approving reviews are required by reviewers with write access. (mergePullRequest)"
            ),
            MergeBlockReason::ReviewRequired
        );
    }

    #[test]
    fn classifies_changes_requested() {
        assert_eq!(
            classify_merge_block("GraphQL: Changes requested by reviewers. (mergePullRequest)"),
            MergeBlockReason::ReviewRequired
        );
    }

    #[test]
    fn classifies_protected_branch_rules() {
        assert_eq!(
            classify_merge_block("Protected branch rules not configured for this branch"),
            MergeBlockReason::BranchProtection
        );
    }

    #[test]
    fn classifies_required_status_check() {
        assert_eq!(
            classify_merge_block(
                "GraphQL: 2 of 3 required status checks are expected. (mergePullRequest)"
            ),
            MergeBlockReason::BranchProtection
        );
    }

    #[test]
    fn classifies_changes_must_be_made_through_a_pull_request() {
        assert_eq!(
            classify_merge_block(
                "GraphQL: Changes must be made through a pull request. (mergePullRequest)"
            ),
            MergeBlockReason::BranchProtection
        );
    }

    #[test]
    fn base_branch_policy_outranks_the_generic_not_mergeable_wording() {
        // gh's client-side precheck emits this one umbrella message for ANY
        // protection rule — a required approving review included — so it
        // classifies as the administrator-clearable class even when a human
        // review is what is actually missing. That ambiguity is exactly why
        // `admin_can_bypass` is a capability hint and callers must consult
        // `pr_review_decision` before reaching for `--admin`.
        assert_eq!(
            classify_merge_block(
                "X Pull request #12 is not mergeable: the base branch policy prohibits the merge."
            ),
            MergeBlockReason::BranchProtection
        );
    }

    #[test]
    fn a_message_naming_a_review_is_never_administrator_clearable() {
        // gh emits either its own precheck refusal or the GraphQL server
        // error, never both, so the classifier only ever sees one of these.
        // Whichever wording arrives, text naming a review must outrank the
        // branch-policy phrasing rather than come back clearable.
        for detail in [
            "GraphQL: At least 1 approving review is required by reviewers with write access. (mergePullRequest)",
            "GraphQL: Changes requested by reviewers. (mergePullRequest)",
        ] {
            let reason = classify_merge_block(detail);
            assert_eq!(reason, MergeBlockReason::ReviewRequired, "for {detail:?}");
            assert!(!reason.admin_can_bypass(), "for {detail:?}");
        }
    }

    #[test]
    fn classifies_review_decision() {
        assert_eq!(
            classify_review_decision(Some("REVIEW_REQUIRED")),
            ReviewDecision::ReviewRequired
        );
        assert_eq!(
            classify_review_decision(Some("CHANGES_REQUESTED")),
            ReviewDecision::ChangesRequested
        );
        assert_eq!(
            classify_review_decision(Some("APPROVED")),
            ReviewDecision::Approved
        );
    }

    #[test]
    fn an_absent_review_decision_means_no_review_is_required() {
        // GitHub returns an empty string when the repository requires no
        // review on this PR — the case that must still allow an --admin retry.
        for raw in [None, Some(""), Some("  "), Some("null")] {
            assert_eq!(
                classify_review_decision(raw),
                ReviewDecision::NotRequired,
                "for {raw:?}"
            );
            assert!(!classify_review_decision(raw).blocks_admin_merge());
        }
    }

    #[test]
    fn an_unrecognized_review_decision_blocks_the_admin_retry() {
        assert!(classify_review_decision(Some("SOMETHING_NEW")).blocks_admin_merge());
    }

    #[test]
    fn only_an_outstanding_review_blocks_the_admin_retry() {
        assert!(ReviewDecision::ReviewRequired.blocks_admin_merge());
        assert!(ReviewDecision::ChangesRequested.blocks_admin_merge());
        assert!(!ReviewDecision::Approved.blocks_admin_merge());
        assert!(!ReviewDecision::NotRequired.blocks_admin_merge());
    }

    #[test]
    fn classifies_dirty_merge_state() {
        assert_eq!(
            classify_merge_block(
                "X Pull request #12 is not mergeable: the merge commit cannot be cleanly created."
            ),
            MergeBlockReason::NotMergeable
        );
    }

    #[test]
    fn classifies_bare_not_mergeable() {
        assert_eq!(
            classify_merge_block("Pull request is not mergeable"),
            MergeBlockReason::NotMergeable
        );
        assert_eq!(
            classify_merge_block("not mergeable"),
            MergeBlockReason::NotMergeable
        );
    }

    #[test]
    fn classifies_merge_conflict() {
        assert_eq!(
            classify_merge_block("merge conflict between the head and base branches"),
            MergeBlockReason::NotMergeable
        );
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            classify_merge_block("AT LEAST 1 APPROVING REVIEW IS REQUIRED"),
            MergeBlockReason::ReviewRequired
        );
        assert_eq!(
            classify_merge_block("REQUIRED STATUS CHECK \"ci\" IS EXPECTED"),
            MergeBlockReason::BranchProtection
        );
    }

    #[test]
    fn unrecognized_text_is_other() {
        for text in [
            "",
            "gh: command failed",
            "HTTP 500: Internal Server Error",
            "GraphQL: Base branch was modified. Review and try the merge again.",
            "X Pull request #12 was already merged",
        ] {
            assert_eq!(
                classify_merge_block(text),
                MergeBlockReason::Other,
                "unexpected classification for {text:?}"
            );
        }
    }

    #[test]
    fn only_branch_protection_is_admin_clearable() {
        assert!(MergeBlockReason::BranchProtection.admin_can_bypass());
        assert!(!MergeBlockReason::ReviewRequired.admin_can_bypass());
        assert!(!MergeBlockReason::NotMergeable.admin_can_bypass());
        assert!(!MergeBlockReason::Other.admin_can_bypass());
    }

    #[test]
    fn detail_prefers_stderr_and_keeps_stdout() {
        assert_eq!(merge_block_detail("", "  boom  \n"), "boom");
        assert_eq!(merge_block_detail("out\n", "err\n"), "err\nout");
        assert_eq!(merge_block_detail("out\n", ""), "out");
        assert_eq!(
            merge_block_detail("  ", "\n"),
            "gh pr merge failed with no output"
        );
    }
}
