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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexExecConfig {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
}

/// Run `codex exec --full-auto` with the given prompt and return the result.
///
/// The prompt is passed as a positional argument (not stdin). The final
/// agent message is captured via the `-o` (output-last-message) flag for
/// reliability, independent of stdout format.
pub async fn codex_exec(prompt: &str, cwd: &Path, timeout: Duration) -> Result<CodexExecResult> {
    codex_exec_with_config(prompt, cwd, timeout, &CodexExecConfig::default()).await
}

pub async fn codex_exec_with_config(
    prompt: &str,
    cwd: &Path,
    timeout: Duration,
    config: &CodexExecConfig,
) -> Result<CodexExecResult> {
    let output_file =
        tempfile::NamedTempFile::new().context("failed to create temp file for codex output")?;
    let output_path = output_file.path().to_path_buf();

    let start = Instant::now();

    let codex = crate::resolve_codex_binary();
    let mut command = Command::new(codex.path());
    command
        .arg("exec")
        .arg("--full-auto")
        .arg("-o")
        .arg(&output_path)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    apply_exec_config(&mut command, config);
    command.arg(prompt);
    for key in crate::process::DAEMON_ENV_STRIP {
        command.env_remove(key);
    }
    crate::apply_codex_binary_env(&mut command, &codex);
    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn codex exec at {}", codex.path().display()))?;

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

fn apply_exec_config(command: &mut Command, config: &CodexExecConfig) {
    if let Some(model) = &config.model {
        command.arg("--model").arg(model);
    }
    for override_arg in config_override_args(config) {
        command.arg("-c").arg(override_arg);
    }
}

fn config_override_args(config: &CodexExecConfig) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(reasoning_effort) = &config.reasoning_effort {
        args.push(format!("model_reasoning_effort={reasoning_effort}"));
    }
    if let Some(service_tier) = &config.service_tier {
        args.push(format!("service_tier={service_tier}"));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_override_args_include_reasoning_and_standard_tier() {
        let args = config_override_args(&CodexExecConfig {
            model: Some("gpt-5.4".into()),
            reasoning_effort: Some("medium".into()),
            service_tier: Some("default".into()),
        });

        assert_eq!(
            args,
            vec![
                "model_reasoning_effort=medium".to_string(),
                "service_tier=default".to_string()
            ]
        );
    }
}
