//! GitHub PR operations via `gh` CLI.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Stdio;

use super::gh_run::run_gh;

// ---------------------------------------------------------------------------
// Internal `gh` response types — private to this module.
// ---------------------------------------------------------------------------

/// Author sub-object returned by `gh pr view --json author`.
#[derive(Debug, Deserialize)]
struct GhAuthor {
    login: Option<String>,
}

/// One entry in `statusCheckRollup`. GitHub returns either a CheckRun
/// (uses `conclusion`) or a StatusContext (uses `state`); both fields are
/// optional so a single struct covers both shapes.
#[derive(Debug, Deserialize)]
struct GhStatusCheck {
    conclusion: Option<String>,
    state: Option<String>,
}

/// One entry in `files` from `gh pr view --json files`.
#[derive(Debug, Deserialize)]
struct GhPrFile {
    path: String,
}

/// Top-level response from `gh pr view --json number,author,body,headRefOid,
/// statusCheckRollup,comments,files`.
#[derive(Debug, Deserialize)]
struct GhPrViewResponse {
    author: Option<GhAuthor>,
    body: Option<String>,
    #[serde(rename = "headRefOid")]
    head_ref_oid: Option<String>,
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<Vec<GhStatusCheck>>,
    /// Only the count matters; each entry is opaque to us.
    comments: Option<Vec<serde_json::Value>>,
    files: Option<Vec<GhPrFile>>,
}

/// Minimal response from `gh pr view --json commits`.
#[derive(Debug, Deserialize)]
struct GhPrCommitsResponse {
    /// Only the count matters; each entry is opaque.
    commits: Option<Vec<serde_json::Value>>,
}

/// One entry from `gh pr list --json url`.
#[derive(Debug, Deserialize)]
struct GhPrListEntry {
    url: String,
}

/// PR status from GitHub.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct PrStatus {
    pub number: String,
    pub author: String,
    pub ci_status: Option<String>,
    pub comments: i64,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub body: String,
    pub head_sha: String,
    pub changed_files: Vec<String>,
}

/// Fetch PR status using `gh pr view`.
pub(crate) async fn fetch_pr_status(repo: &str, pr_number: &str) -> Result<PrStatus> {
    let text = run_gh(&[
        "pr",
        "view",
        pr_number,
        "--repo",
        repo,
        "--json",
        "number,author,body,headRefOid,statusCheckRollup,comments,files",
    ])
    .await?;
    let parsed: GhPrViewResponse = serde_json::from_str(&text).context("parse gh pr view JSON")?;

    let author = parsed.author.and_then(|a| a.login).unwrap_or_default();
    let body = parsed.body.unwrap_or_default();
    let head_sha = parsed.head_ref_oid.unwrap_or_default();

    // StatusCheckRollup contains both CheckRun (uses `conclusion`) and
    // StatusContext (uses `state`). Normalize both into a single status.
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

    // Thread counts come from get_pr_review_threads (GraphQL) in fetch_pr_data,
    // not from gh pr view. Set to zero here; the caller overrides with hygiene data.
    Ok(PrStatus {
        number: pr_number.to_string(),
        author,
        ci_status,
        comments,
        unresolved_threads: 0,
        unreplied_threads: 0,
        unaddressed_issue_comments: 0,
        body,
        head_sha,
        changed_files,
    })
}

/// Squash-merge a PR.
#[allow(dead_code)]
pub async fn merge_pr(repo: &str, pr_number: &str) -> Result<String> {
    run_gh(&["pr", "merge", pr_number, "--repo", repo, "--squash"]).await
}

/// Check if a PR is already merged on GitHub. Returns an error on gh
/// failure so callers can distinguish transient failures from "not merged".
pub(crate) async fn is_pr_merged(repo: &str, pr_number: &str) -> Result<bool> {
    let state = run_gh(&[
        "pr", "view", pr_number, "--repo", repo, "--json", "state", "-q", ".state",
    ])
    .await?;
    Ok(state.trim().eq_ignore_ascii_case("MERGED"))
}

/// Check if branch is ahead of main.
pub(crate) async fn is_pr_branch_ahead(repo: &str, pr_number: &str) -> Result<bool> {
    let text = run_gh(&["pr", "view", pr_number, "--repo", repo, "--json", "commits"]).await?;
    let parsed: GhPrCommitsResponse =
        serde_json::from_str(&text).context("parse gh pr view commits JSON")?;
    let commits = parsed.commits.as_deref().map(|a| a.len()).unwrap_or(0);
    Ok(commits > 0)
}

/// Close an open PR without merging.
pub async fn close_pr(repo: &str, pr_number: &str) -> Result<()> {
    run_gh(&["pr", "close", pr_number, "--repo", repo]).await?;
    Ok(())
}

/// Discover an open PR for a branch. Returns the PR number if found.
pub(crate) async fn discover_pr_for_branch(repo: &str, branch: &str) -> Option<i64> {
    let output = match tokio::process::Command::new("gh")
        .args([
            "pr", "list", "--repo", repo, "--head", branch, "--state", "open", "--json", "url",
            "--limit", "1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(module = "github", repo = %repo, branch = %branch, error = %e, "failed to execute gh pr list");
            return None;
        }
    };

    if !output.status.success() {
        // Stderr is display-only for the log, so lossy is fine here.
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(module = "github", repo = %repo, branch = %branch, stderr = %stderr, "gh pr list failed");
        return None;
    }
    let text = match String::from_utf8(output.stdout) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(module = "github", repo = %repo, branch = %branch, error = %e, "gh pr list stdout not UTF-8");
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
    arr.first().and_then(|v| crate::parse_pr_number(&v.url))
}
