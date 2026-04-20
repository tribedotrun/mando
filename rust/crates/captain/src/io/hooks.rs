//! Hook lifecycle — pre_spawn, worker_teardown, post_merge.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Result};

/// Run a named hook script.
///
/// When `fatal_on_failure` is true, a non-zero exit status is returned as an
/// error so the caller can abort the surrounding operation. When false, the
/// failure is logged at warn level and `Ok(())` is returned.
pub(crate) async fn run_hook(
    hooks: &HashMap<String, String>,
    hook_name: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
    fatal_on_failure: bool,
) -> Result<()> {
    let script = match hooks.get(hook_name) {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(()), // No hook configured — no-op.
    };

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c").arg(script).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let snippet: String = stderr.chars().take(200).collect();
        if fatal_on_failure {
            return Err(anyhow!(
                "hook `{}` failed (exit {:?}): {}",
                hook_name,
                output.status.code(),
                snippet
            ));
        }
        tracing::warn!(
            module = "captain-io-hooks",
            hook = hook_name,
            "hook failed (non-fatal): {}",
            snippet
        );
    }
    Ok(())
}

/// Run the pre_spawn hook (fatal — aborts spawn on failure).
pub(crate) async fn pre_spawn(
    hooks: &HashMap<String, String>,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<()> {
    run_hook(hooks, "pre_spawn", cwd, env, true).await
}

/// Run the post_merge hook (fatal — aborts the surrounding merge op).
pub(crate) async fn post_merge(
    hooks: &HashMap<String, String>,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<()> {
    run_hook(hooks, "post_merge", cwd, env, true).await
}
