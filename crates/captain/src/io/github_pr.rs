//! Richer GitHub PR data fetching via `gh` CLI.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::gh_run::{run_gh, run_gh_api_paginate};

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

/// Lenient author deserializer. Handles null, missing, and object values
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

/// Fetch the PR description body.
pub async fn get_pr_body(repo: &str, pr: u32) -> Result<String> {
    let endpoint = format!("repos/{repo}/pulls/{pr}");
    let body = run_gh(&["api", &endpoint, "--jq", ".body"]).await?;
    let trimmed = body.trim();
    // gh api --jq outputs literal "null" for null JSON values
    if trimmed == "null" || trimmed.is_empty() {
        return Ok(String::new());
    }
    Ok(trimmed.to_string())
}

/// Fetch all issue comments on a PR.
///
/// Returns `Err` when the API returns raw comment rows but every single one
/// fails to parse, because that's indistinguishable from a schema drift
/// that would make the captain ship a PR while thinking there are no
/// comments. The caller is expected to mark the PR as degraded.
/// Individual parse failures (e.g. one deleted-user comment among many) are
/// still skipped with a warn log so one bad row does not block the whole fetch.
pub(crate) async fn get_pr_comments(repo: &str, pr: u32) -> Result<Vec<PrComment>> {
    let endpoint = format!("repos/{repo}/issues/{pr}/comments");
    let items = run_gh_api_paginate(&[&endpoint]).await?;
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
        return Err(anyhow::anyhow!(
            "all {total} PR comments failed to parse for pr #{pr} in {repo}, possible API schema drift"
        ));
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

    let query_arg = format!("query={query}");
    let text = run_gh(&["api", "graphql", "-f", &query_arg]).await?;
    let val: serde_json::Value = serde_json::from_str(&text)?;

    let threads_arr = &val["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"];
    let threads = threads_arr
        .as_array()
        .context("expected reviewThreads.nodes array")?;

    let mut result = Vec::new();
    for thread in threads {
        let id = thread["id"].as_str().unwrap_or("").to_string();
        // A missing isResolved field must NOT silently become false. Callers
        // should mark the PR as degraded.
        let is_resolved = thread["isResolved"].as_bool().with_context(|| {
            format!(
                "missing isResolved on thread {} (pr #{pr}, repo {repo})",
                if id.is_empty() { "<unknown>" } else { &id }
            )
        })?;

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
