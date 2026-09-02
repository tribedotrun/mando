use anyhow::{Context, Result};
use global_infra::retry::{classify_cli_error, retry_on_transient, RetryConfig};
use std::path::{Path, PathBuf};

fn gh_retry_config() -> RetryConfig {
    RetryConfig::default()
}

pub(crate) async fn run_gh(args: &[&str]) -> Result<String> {
    run_gh_with_cwd(None, args).await
}

pub(crate) async fn run_gh_in_dir(cwd: &Path, args: &[&str]) -> Result<String> {
    run_gh_with_cwd(Some(cwd), args).await
}

/// Run a read-only `gh` command whose successful stdout is binary data.
pub(crate) async fn run_gh_bytes(args: &[&str]) -> Result<Vec<u8>> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let owned = owned.clone();
            async move {
                let str_refs: Vec<&str> = owned.iter().map(String::as_str).collect();
                let output = spawn_gh(None, &str_refs).await?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let cmd = owned.first().cloned().unwrap_or_default();
                    anyhow::bail!("gh {} failed: {}", cmd, stderr.trim());
                }
                Ok(output.stdout)
            }
        },
    )
    .await
}

/// Raw result of a single `gh` invocation, non-zero exit included.
pub(crate) struct GhOutput {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

async fn spawn_gh(cwd: Option<&Path>, args: &[&str]) -> Result<std::process::Output> {
    let mut command = tokio::process::Command::new("gh");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .output()
        .await
        .with_context(|| format!("gh {}", args.first().copied().unwrap_or_default()))
}

async fn run_gh_with_cwd(cwd: Option<&Path>, args: &[&str]) -> Result<String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let cwd: Option<PathBuf> = cwd.map(Path::to_path_buf);
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let owned = owned.clone();
            let cwd = cwd.clone();
            async move {
                let str_refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
                let output = spawn_gh(cwd.as_deref(), &str_refs).await?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let cmd = owned.first().cloned().unwrap_or_default();
                    anyhow::bail!("gh {} failed: {}", cmd, stderr.trim());
                }
                String::from_utf8(output.stdout).context("gh output not UTF-8")
            }
        },
    )
    .await
}

/// Run `gh` exactly once and hand back the raw result, treating a non-zero
/// exit as data rather than an error.
///
/// Used by state-changing commands whose refusal text is the product (merge
/// blocks). Deliberately unretried: retrying a write is the caller's call, and
/// gh's refusal message is what needs classifying, not an `anyhow` chain.
pub(crate) async fn run_gh_capture(args: &[&str]) -> Result<GhOutput> {
    let output = spawn_gh(None, args).await?;
    Ok(GhOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub(crate) async fn run_gh_api_paginate(args: &[&str]) -> Result<Vec<serde_json::Value>> {
    let mut full: Vec<&str> = vec!["api", "--paginate"];
    full.extend_from_slice(args);
    let text = run_gh(&full).await?;

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
