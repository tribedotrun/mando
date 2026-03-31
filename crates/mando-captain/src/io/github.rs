//! GitHub PR operations via `gh` CLI.

use anyhow::{Context, Result};
use mando_shared::retry::{classify_cli_error, retry_on_transient};
use std::process::Stdio;

use super::gh_retry_config;

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
    let repo = repo.to_string();
    let pr_number = pr_number.to_string();
    let text = retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let repo = repo.clone();
            let pr_number = pr_number.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args([
                        "pr",
                        "view",
                        &pr_number,
                        "--repo",
                        &repo,
                        "--json",
                        "number,author,body,headRefOid,statusCheckRollup,comments,files",
                    ])
                    .output()
                    .await
                    .context("gh pr view")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh pr view failed: {}", stderr);
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
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

    // Thread counts come from get_pr_review_threads (GraphQL) in fetch_pr_data,
    // not from gh pr view. Set to zero here — the caller overrides with hygiene data.
    let unresolved = 0i64;
    let unreplied = 0i64;

    let changed_files = val["files"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f["path"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(PrStatus {
        number: pr_number,
        author,
        ci_status,
        comments,
        unresolved_threads: unresolved,
        unreplied_threads: unreplied,
        unaddressed_issue_comments: 0,
        body,
        head_sha,
        changed_files,
    })
}

/// Squash-merge a PR.
pub async fn merge_pr(repo: &str, pr_number: &str) -> Result<String> {
    let repo = repo.to_string();
    let pr_number = pr_number.to_string();
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let repo = repo.clone();
            let pr_number = pr_number.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args(["pr", "merge", &pr_number, "--repo", &repo, "--squash"])
                    .output()
                    .await
                    .context("gh pr merge")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh pr merge failed: {}", stderr);
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
    .await
}

/// Check if branch is ahead of main.
pub(crate) async fn is_pr_branch_ahead(repo: &str, pr_number: &str) -> Result<bool> {
    let repo = repo.to_string();
    let pr_number = pr_number.to_string();
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let repo = repo.clone();
            let pr_number = pr_number.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args([
                        "pr", "view", &pr_number, "--repo", &repo, "--json", "commits",
                    ])
                    .output()
                    .await
                    .context("gh pr view commits")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh pr view commits failed: {}", stderr);
                }
                let text = String::from_utf8_lossy(&output.stdout);
                let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to parse gh pr view commits JSON");
                    serde_json::Value::default()
                });
                let commits = val["commits"].as_array().map(|a| a.len()).unwrap_or(0);
                Ok(commits > 0)
            }
        },
    )
    .await
}

/// Close an open PR without merging.
pub(crate) async fn close_pr(repo: &str, pr_number: &str) -> Result<()> {
    let repo = repo.to_string();
    let pr_number = pr_number.to_string();
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let repo = repo.clone();
            let pr_number = pr_number.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args(["pr", "close", &pr_number, "--repo", &repo])
                    .output()
                    .await
                    .context("gh pr close")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh pr close failed: {}", stderr);
                }
                Ok(())
            }
        },
    )
    .await
}

/// Discover an open PR for a branch. Returns the PR URL if found.
pub(crate) async fn discover_pr_for_branch(repo: &str, branch: &str) -> Option<String> {
    let output = tokio::process::Command::new("gh")
        .args([
            "pr", "list", "--repo", repo, "--head", branch, "--state", "open", "--json", "url",
            "--limit", "1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let arr: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    arr.first()
        .and_then(|v| v["url"].as_str())
        .and_then(mando_types::task::normalize_pr)
}
