//! Linear CLI wrapper — thin subprocess calls.

use std::path::Path;

use anyhow::{bail, Context, Result};
use mando_shared::retry::{classify_cli_error, retry_on_transient, RetryConfig};

fn linear_retry_config() -> RetryConfig {
    RetryConfig::default()
}

const CLI_PATH: &str = "~/.claude/skills/mando-linear/linear";

/// Resolve the Linear CLI binary path.
///
/// Uses the configured path if set, otherwise `~/.claude/skills/mando-linear/linear`.
/// Fails fast if the resolved path doesn't exist.
pub(crate) fn resolve_cli_path(configured: &str) -> Result<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let raw = if !configured.is_empty() {
        configured.to_string()
    } else {
        CLI_PATH.to_string()
    };
    let path = raw.replace('~', &home);
    if !Path::new(&path).exists() {
        bail!("linear CLI not found at {path} — is Mando installed correctly?");
    }
    Ok(path)
}

/// Update Linear issue status.
pub(crate) async fn update_status(cli_path: &str, issue_id: &str, status: &str) -> Result<()> {
    let cli_path = cli_path.to_string();
    let issue_id = issue_id.to_string();
    let status = status.to_string();
    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let issue_id = issue_id.clone();
            let status = status.clone();
            async move {
                let output = tokio::process::Command::new(&cli_path)
                    .args(["status", &issue_id, &status])
                    .output()
                    .await
                    .context("linear status")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear status failed: {}", stderr);
                }
                Ok(())
            }
        },
    )
    .await
}

/// Post a comment on a Linear issue, returning the comment UUID.
pub(crate) async fn post_comment(cli_path: &str, issue_id: &str, body: &str) -> Result<String> {
    let cli_path = cli_path.to_string();
    let issue_id = issue_id.to_string();
    let body = body.to_string();
    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let issue_id = issue_id.clone();
            let body = body.clone();
            async move {
                let output = tokio::process::Command::new(&cli_path)
                    .args(["comment", &issue_id, &body])
                    .output()
                    .await
                    .context("linear comment")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear comment failed: {}", stderr);
                }
                let text = String::from_utf8_lossy(&output.stdout);
                let comment_id = text
                    .split("id=")
                    .nth(1)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                Ok(comment_id)
            }
        },
    )
    .await
}

/// Update an existing comment by UUID.
pub(crate) async fn update_comment(cli_path: &str, comment_id: &str, body: &str) -> Result<()> {
    let cli_path = cli_path.to_string();
    let comment_id = comment_id.to_string();
    let body = body.to_string();
    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let comment_id = comment_id.clone();
            let body = body.clone();
            async move {
                let output = tokio::process::Command::new(&cli_path)
                    .args(["comment-update", &comment_id, &body])
                    .output()
                    .await
                    .context("linear comment-update")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear comment-update failed: {}", stderr);
                }
                Ok(())
            }
        },
    )
    .await
}

/// Search Linear issues by title.
pub(crate) async fn search_issues(cli_path: &str, query: &str) -> Result<Vec<String>> {
    let cli_path = cli_path.to_string();
    let query = query.to_string();
    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let query = query.clone();
            async move {
                let output = tokio::process::Command::new(&cli_path)
                    .args(["search", &query])
                    .output()
                    .await
                    .context("linear search")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear search failed: {}", stderr);
                }
                let text = String::from_utf8_lossy(&output.stdout);
                Ok(text.lines().map(String::from).collect())
            }
        },
    )
    .await
}

/// Create a Linear issue.
pub(crate) async fn create_issue(
    cli_path: &str,
    team: &str,
    title: &str,
    description: Option<&str>,
    labels: &[String],
) -> Result<String> {
    let cli_path = cli_path.to_string();
    let mut args = vec!["create".to_string(), team.to_string(), title.to_string()];
    if let Some(desc) = description {
        args.push("-d".to_string());
        args.push(desc.to_string());
    }
    for label in labels {
        args.push("-l".to_string());
        args.push(label.clone());
    }

    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let args = args.clone();
            async move {
                let str_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let output = tokio::process::Command::new(&cli_path)
                    .args(&str_refs)
                    .output()
                    .await
                    .context("linear create")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear create failed: {}", stderr);
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
    .await
}
