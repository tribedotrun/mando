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

/// Run a Linear CLI command with retry. Returns stdout on success.
async fn run_linear_cli(cli_path: &str, args: &[&str]) -> Result<String> {
    let cli_path = cli_path.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let label = args.first().cloned().unwrap_or_default();
    retry_on_transient(
        &linear_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let cli_path = cli_path.clone();
            let args = args.clone();
            let label = label.clone();
            async move {
                let str_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let output = tokio::process::Command::new(&cli_path)
                    .args(&str_refs)
                    .output()
                    .await
                    .with_context(|| format!("linear {label}"))?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("linear {label} failed: {stderr}");
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        },
    )
    .await
}

/// Update Linear issue status.
pub(crate) async fn update_status(cli_path: &str, issue_id: &str, status: &str) -> Result<()> {
    run_linear_cli(cli_path, &["status", issue_id, status]).await?;
    Ok(())
}

/// Post a comment on a Linear issue, returning the comment UUID.
pub(crate) async fn post_comment(cli_path: &str, issue_id: &str, body: &str) -> Result<String> {
    let text = run_linear_cli(cli_path, &["comment", issue_id, body]).await?;
    Ok(text
        .split("id=")
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_default())
}

/// Update an existing comment by UUID.
pub(crate) async fn update_comment(cli_path: &str, comment_id: &str, body: &str) -> Result<()> {
    run_linear_cli(cli_path, &["comment-update", comment_id, body]).await?;
    Ok(())
}

/// Search Linear issues by title.
pub(crate) async fn search_issues(cli_path: &str, query: &str) -> Result<Vec<String>> {
    let text = run_linear_cli(cli_path, &["search", query]).await?;
    Ok(text.lines().map(String::from).collect())
}

/// Create a Linear issue.
pub(crate) async fn create_issue(
    cli_path: &str,
    team: &str,
    title: &str,
    description: Option<&str>,
    labels: &[String],
) -> Result<String> {
    let mut owned_args: Vec<String> = vec!["create".into(), team.into(), title.into()];
    if let Some(desc) = description {
        owned_args.push("-d".into());
        owned_args.push(desc.into());
    }
    for label in labels {
        owned_args.push("-l".into());
        owned_args.push(label.clone());
    }
    let args: Vec<&str> = owned_args.iter().map(|s| s.as_str()).collect();
    run_linear_cli(cli_path, &args).await
}
