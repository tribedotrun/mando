use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use agent_runtime_core::ChildLifetime;
use anyhow::{Context, Result};
use global_types::Pid;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::time::timeout;

use crate::stream::{read_until_session_id, OpenCodeEvent};

/// OpenCode `run` invocation for any Mando stage.
pub struct OpenCodeRunConfig<'a> {
    pub cwd: &'a Path,
    pub prompt: &'a str,
    pub model: &'a str,
    pub variant: Option<&'a str>,
    pub resume_session_id: Option<&'a str>,
}

/// Running OpenCode process plus the already-buffered startup events.
pub struct StartedOpenCodeRun {
    pub session_id: String,
    pub pid: Pid,
    pub child: Child,
    pub stdout_lines: Lines<BufReader<ChildStdout>>,
    pub stderr: Option<ChildStderr>,
    pub buffered_events: Vec<OpenCodeEvent>,
}

pub async fn spawn_run(
    config: &OpenCodeRunConfig<'_>,
    session_start_timeout: Duration,
) -> Result<StartedOpenCodeRun> {
    ensure_binary_available().await?;
    let mut child =
        agent_runtime_core::spawn_isolated(opencode_command(config), ChildLifetime::KillOnDrop)
            .context("spawn opencode run")?;
    let pid = child.id().map(Pid::new).unwrap_or_else(|| Pid::new(0));
    let stdout = child
        .stdout
        .take()
        .context("opencode stdout was not piped")?;
    let stderr = child.stderr.take();
    let mut stdout_lines = BufReader::new(stdout).lines();
    let (session_id, buffered_events) = timeout(
        session_start_timeout,
        read_until_session_id(&mut stdout_lines, config.resume_session_id),
    )
    .await
    .context("timed out waiting for OpenCode session id")??;

    Ok(StartedOpenCodeRun {
        session_id,
        pid,
        child,
        stdout_lines,
        stderr,
        buffered_events,
    })
}

pub async fn ensure_binary_available() -> Result<()> {
    let found = Command::new("which")
        .arg("opencode")
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !found {
        anyhow::bail!("opencode binary not found on PATH");
    }
    Ok(())
}

pub async fn terminate_process(pid: Pid) -> Result<()> {
    if pid.as_u32() > 0 && agent_runtime_core::is_process_alive(pid) {
        agent_runtime_core::kill_process(pid).await?;
    }
    Ok(())
}

fn opencode_command(config: &OpenCodeRunConfig<'_>) -> Command {
    let mut command = Command::new("opencode");
    command
        .arg("run")
        .arg("--format")
        .arg("json")
        .arg("--model")
        .arg(config.model)
        .arg("--dir")
        .arg(config.cwd)
        .arg("--dangerously-skip-permissions");
    if let Some(variant) = config.variant.filter(|value| !value.is_empty()) {
        command.arg("--variant").arg(variant);
    }
    if let Some(session_id) = config.resume_session_id {
        command.arg("--session").arg(session_id);
    }
    command
        .arg(config.prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .current_dir(config.cwd);
    command
}
