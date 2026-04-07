//! GitHub PR operations via `gh` CLI.

use anyhow::{Context, Result};
use std::process::Stdio;

use super::gh_run::run_gh;

/// PR status from GitHub.
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
    let val: serde_json::Value = serde_json::from_str(&text).context("parse gh pr view JSON")?;

    let author = val["author"]["login"].as_str().unwrap_or("").to_string();
    let body = val["body"].as_str().unwrap_or("").to_string();
    let head_sha = val["headRefOid"].as_str().unwrap_or("").to_string();

    // StatusCheckRollup contains both CheckRun (uses `conclusion`) and
    // StatusContext (uses `state`). Normalize both into a single status.
    let ci_status = val["statusCheckRollup"].as_array().map(|arr| {
        let is_failure = |c: &serde_json::Value| -> bool {
            let s = c["conclusion"]
                .as_str()
                .or_else(|| c["state"].as_str())
                .unwrap_or("PENDING");
            matches!(
                s,
                "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
            )
        };
        let is_success = |c: &serde_json::Value| -> bool {
            let s = c["conclusion"]
                .as_str()
                .or_else(|| c["state"].as_str())
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

    let comments = val["comments"]
        .as_array()
        .map(|a| a.len() as i64)
        .unwrap_or(0);

    let changed_files = val["files"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f["path"].as_str().map(String::from))
                .collect()
        })
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
    let val: serde_json::Value =
        serde_json::from_str(&text).context("parse gh pr view commits JSON")?;
    let commits = val["commits"].as_array().map(|a| a.len()).unwrap_or(0);
    Ok(commits > 0)
}

/// Close an open PR without merging.
pub async fn close_pr(repo: &str, pr_number: &str) -> Result<()> {
    run_gh(&["pr", "close", pr_number, "--repo", repo]).await?;
    Ok(())
}

/// Discover an open PR for a branch. Returns the PR URL if found.
pub(crate) async fn discover_pr_for_branch(repo: &str, branch: &str) -> Option<String> {
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
    let arr: Vec<serde_json::Value> = match serde_json::from_str(&text) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(module = "github", repo = %repo, branch = %branch, error = %e, "failed to parse gh pr list JSON");
            return None;
        }
    };
    arr.first()
        .and_then(|v| v["url"].as_str())
        .and_then(mando_types::task::normalize_pr)
}
