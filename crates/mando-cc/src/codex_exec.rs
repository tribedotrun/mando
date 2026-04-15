//! Headless Codex CLI wrapper for one-shot execution.
//!
//! Spawns `codex exec --full-auto` as a subprocess, captures the final text
//! output, and returns it with timing metadata. Designed for use as a
//! feedback agent in the planning pipeline.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::process::Command;

/// Result from a headless Codex execution.
pub struct CodexExecResult {
    /// Final text output from the agent.
    pub text: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Run `codex exec --full-auto` with the given prompt and return the result.
///
/// The prompt is passed as a positional argument (not stdin). The final
/// agent message is captured via the `-o` (output-last-message) flag for
/// reliability, independent of stdout format.
pub async fn codex_exec(prompt: &str, cwd: &Path, timeout: Duration) -> Result<CodexExecResult> {
    let output_file =
        tempfile::NamedTempFile::new().context("failed to create temp file for codex output")?;
    let output_path = output_file.path().to_path_buf();

    let start = Instant::now();

    let child = Command::new("codex")
        .arg("exec")
        .arg("--full-auto")
        .arg("-o")
        .arg(&output_path)
        .arg(prompt)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn codex exec")?;

    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => anyhow::bail!("codex exec IO error: {e}"),
        Err(_) => anyhow::bail!("codex exec timed out after {}s", timeout.as_secs()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "codex exec exited with {}: {}",
            output.status,
            stderr.chars().take(500).collect::<String>()
        );
    }

    let text = tokio::fs::read_to_string(&output_path)
        .await
        .context("failed to read codex output file")?;

    if text.trim().is_empty() {
        anyhow::bail!("codex exec produced empty output");
    }

    Ok(CodexExecResult { text, duration_ms })
}
