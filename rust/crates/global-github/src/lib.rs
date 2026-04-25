//! GitHub CLI/API provider boundary.
//!
//! This crate is the only production code allowed to spawn `gh`. Callers own
//! orchestration policy; this crate owns command execution, retries, upstream
//! JSON parsing, and typed GitHub response shapes.

mod command;
mod review_threads;
mod types;

use anyhow::{Context, Result};
use command::{run_gh, run_gh_api_paginate, run_gh_in_dir};
use serde::Deserialize;
use std::path::Path;

pub use review_threads::get_pr_review_threads;
pub use types::{MergeableStatus, PrComment, PrState, PrStatus, ReviewThread, ThreadComment};

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

pub async fn fetch_pr_status(repo: &str, pr_number: &str) -> Result<PrStatus> {
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

#[allow(dead_code)]
pub async fn merge_pr(repo: &str, pr_number: &str) -> Result<String> {
    run_gh(&["pr", "merge", pr_number, "--repo", repo, "--squash"]).await
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
