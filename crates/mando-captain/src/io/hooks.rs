//! Hook lifecycle — pre_spawn, worker_teardown, post_merge.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

/// Run a named hook script.
pub(crate) async fn run_hook(
    hooks: &HashMap<String, String>,
    hook_name: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
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
        tracing::warn!(
            hook = hook_name,
            "hook failed (non-fatal): {}",
            stderr.chars().take(200).collect::<String>()
        );
    }
    Ok(())
}

/// Run the pre_spawn hook.
pub(crate) async fn pre_spawn(
    hooks: &HashMap<String, String>,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<()> {
    run_hook(hooks, "pre_spawn", cwd, env).await
}

/// Run the worker_teardown hook (no env overrides).
pub(crate) async fn teardown(hooks: &HashMap<String, String>, cwd: &Path) -> Result<()> {
    run_hook(hooks, "worker_teardown", cwd, &HashMap::new()).await
}

/// Run the post_merge hook.
pub(crate) async fn post_merge(
    hooks: &HashMap<String, String>,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<()> {
    run_hook(hooks, "post_merge", cwd, env).await
}
