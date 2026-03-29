//! Richer GitHub PR data fetching via `gh` CLI.

use anyhow::{Context, Result};
use mando_shared::retry::{classify_cli_error, retry_on_transient};
use serde::Deserialize;

use super::gh_retry_config;

/// A comment on a PR (issue comment).
///
/// The REST API returns `user` + `created_at`; use `alias` so both shapes work.
#[derive(Debug, Clone, Deserialize)]
pub struct PrComment {
    #[serde(default)]
    pub id: u64,
    #[serde(alias = "author", deserialize_with = "deserialize_author_lenient")]
    pub user: String,
    #[serde(default)]
    pub body: String,
    #[serde(alias = "createdAt", default)]
    pub created_at: String,
}

/// A comment within a review thread.
#[derive(Debug, Clone)]
pub struct ThreadComment {
    pub author: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
}

/// A review thread with resolution status.
#[derive(Debug, Clone)]
pub struct ReviewThread {
    pub id: String,
    pub is_resolved: bool,
    pub comments: Vec<ThreadComment>,
}

/// Lenient author deserializer — handles null, missing, and object values
/// (returns empty string on failure).
fn deserialize_author_lenient<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match val {
        Some(serde_json::Value::Object(map)) => map
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(serde_json::Value::String(s)) => s,
        _ => String::new(),
    })
}

async fn gh_api_paginate(args: &[&str]) -> Result<Vec<serde_json::Value>> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let text = retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let args = args.clone();
            async move {
                let str_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let output = tokio::process::Command::new("gh")
                    .args(["api", "--paginate"])
                    .args(&str_refs)
                    .output()
                    .await
                    .context("gh api")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh api failed: {stderr}");
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
    .await?;

    let mut results = Vec::new();
    for chunk in text.split('\n') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let val: serde_json::Value = serde_json::from_str(chunk)?;
        if let serde_json::Value::Array(arr) = val {
            results.extend(arr);
        } else {
            results.push(val);
        }
    }
    Ok(results)
}

/// Fetch the PR description body.
pub async fn get_pr_body(repo: &str, pr: u32) -> Result<String> {
    let endpoint = format!("repos/{repo}/pulls/{pr}");
    let text = retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let endpoint = endpoint.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args(["api", &endpoint, "--jq", ".body"])
                    .output()
                    .await
                    .context("gh api pr body")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh api failed: {stderr}");
                }
                let body = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // gh api --jq outputs literal "null" for null JSON values
                if body == "null" || body.is_empty() {
                    return Ok(String::new());
                }
                Ok(body)
            }
        },
    )
    .await?;
    Ok(text)
}

/// Fetch all issue comments on a PR.
pub(crate) async fn get_pr_comments(repo: &str, pr: u32) -> Result<Vec<PrComment>> {
    let endpoint = format!("repos/{repo}/issues/{pr}/comments");
    let items = gh_api_paginate(&[&endpoint]).await?;
    let total = items.len();
    let comments: Vec<PrComment> = items
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<PrComment>(v) {
            Ok(c) if !c.user.is_empty() => Some(c),
            Ok(_) => None, // skip comments with empty/null user (deleted accounts, system events)
            Err(e) => {
                tracing::warn!(pr = pr, error = %e, "skipping unparseable PR comment");
                None
            }
        })
        .collect();
    if comments.is_empty() && total > 0 {
        tracing::error!(
            pr = pr,
            total_raw = total,
            "all PR comments failed to parse — possible API schema change"
        );
    }
    Ok(comments)
}

/// Fetch review threads with resolution status via GraphQL.
pub(crate) async fn get_pr_review_threads(repo: &str, pr: u32) -> Result<Vec<ReviewThread>> {
    let (owner, name) = repo
        .split_once('/')
        .context("repo must be owner/name format")?;

    let query = format!(
        r#"query {{
  repository(owner: "{owner}", name: "{name}") {{
    pullRequest(number: {pr}) {{
      reviewThreads(first: 100) {{
        nodes {{
          id
          isResolved
          comments(first: 100) {{
            nodes {{
              author {{ login }}
              body
              path
              line
            }}
          }}
        }}
      }}
    }}
  }}
}}"#
    );

    let text = retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let query = query.clone();
            async move {
                let output = tokio::process::Command::new("gh")
                    .args(["api", "graphql", "-f", &format!("query={query}")])
                    .output()
                    .await
                    .context("gh api graphql")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("gh api graphql failed: {stderr}");
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
    .await?;
    let val: serde_json::Value = serde_json::from_str(&text)?;

    let threads_arr = &val["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"];
    let threads = threads_arr
        .as_array()
        .context("expected reviewThreads.nodes array")?;

    let mut result = Vec::new();
    for thread in threads {
        let id = thread["id"].as_str().unwrap_or("").to_string();
        let is_resolved = thread["isResolved"].as_bool().unwrap_or(false);

        let comments = thread["comments"]["nodes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|c| ThreadComment {
                        author: c["author"]["login"].as_str().unwrap_or("").to_string(),
                        body: c["body"].as_str().unwrap_or("").to_string(),
                        path: c["path"].as_str().map(String::from),
                        line: c["line"].as_u64().map(|n| n as u32),
                    })
                    .collect()
            })
            .unwrap_or_default();

        result.push(ReviewThread {
            id,
            is_resolved,
            comments,
        });
    }

    Ok(result)
}
