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
                let mut command = tokio::process::Command::new("gh");
                command.args(&str_refs);
                if let Some(cwd) = cwd.as_deref() {
                    command.current_dir(cwd);
                }
                let output = command.output().await.with_context(|| {
                    format!("gh {}", owned.first().cloned().unwrap_or_default())
                })?;
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
