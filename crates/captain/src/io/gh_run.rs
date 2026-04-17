//! Shared `gh` CLI runner used by github.rs and github_pr.rs.
//!
//! Wraps `retry_on_transient` + spawn + stderr-as-error + stdout-as-string
//! so each caller is a one-liner.

use anyhow::{Context, Result};
use global_infra::retry::{classify_cli_error, retry_on_transient};

use super::gh_retry_config;

/// Run `gh <args>` with transient-error retries. Returns stdout on success,
/// an error containing stderr on failure.
pub(crate) async fn run_gh(args: &[&str]) -> Result<String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    retry_on_transient(
        &gh_retry_config(),
        |e: &anyhow::Error| classify_cli_error(&e.to_string()),
        || {
            let owned = owned.clone();
            async move {
                let str_refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
                let output = tokio::process::Command::new("gh")
                    .args(&str_refs)
                    .output()
                    .await
                    .with_context(|| {
                        format!("gh {}", owned.first().cloned().unwrap_or_default())
                    })?;
                if !output.status.success() {
                    // Stderr is display-only for the error message, so lossy is fine here.
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

/// Run `gh api --paginate <args>` and return a Vec of JSON values. The output
/// is newline-delimited; each non-empty line is parsed as a JSON value. Arrays
/// are flattened.
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
