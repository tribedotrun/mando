use crate::command::run_gh;
use crate::types::{ReviewThread, ThreadComment};
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GhGraphQlReviewResponse {
    data: GhGraphQlReviewData,
}

#[derive(Debug, Deserialize)]
struct GhGraphQlReviewData {
    repository: GhReviewRepository,
}

#[derive(Debug, Deserialize)]
struct GhReviewRepository {
    #[serde(rename = "pullRequest")]
    pull_request: GhReviewPullRequest,
}

#[derive(Debug, Deserialize)]
struct GhReviewPullRequest {
    #[serde(rename = "reviewThreads")]
    review_threads: GhReviewThreadsConnection,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadsConnection {
    nodes: Vec<GhReviewThreadNode>,
}

#[derive(Debug, Deserialize)]
struct GhReviewThreadNode {
    id: String,
    #[serde(rename = "isResolved")]
    is_resolved: Option<bool>,
    comments: GhReviewCommentsConnection,
}

#[derive(Debug, Deserialize)]
struct GhReviewCommentsConnection {
    nodes: Vec<GhReviewCommentNode>,
}

#[derive(Debug, Deserialize)]
struct GhReviewCommentNode {
    author: Option<GhAuthorLogin>,
    body: Option<String>,
    path: Option<String>,
    line: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GhAuthorLogin {
    login: Option<String>,
}

pub async fn get_pr_review_threads(repo: &str, pr: u32) -> Result<Vec<ReviewThread>> {
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
    let response: GhGraphQlReviewResponse =
        serde_json::from_str(&text).context("parse GraphQL reviewThreads response")?;

    let mut result = Vec::new();
    for thread in response.data.repository.pull_request.review_threads.nodes {
        let id = thread.id;
        let is_resolved = thread.is_resolved.with_context(|| {
            format!(
                "missing isResolved on thread {} (pr #{pr}, repo {repo})",
                if id.is_empty() { "<unknown>" } else { &id }
            )
        })?;

        let comments = thread
            .comments
            .nodes
            .into_iter()
            .map(|c| ThreadComment {
                author: c.author.and_then(|a| a.login).unwrap_or_default(),
                body: c.body.unwrap_or_default(),
                path: c.path,
                line: c.line,
            })
            .collect();

        result.push(ReviewThread {
            id,
            is_resolved,
            comments,
        });
    }

    Ok(result)
}
