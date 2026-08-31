use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_runtime_core::ChildLifetime;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};

use crate::types::StderrTail;

const STDERR_TAIL_LINES: usize = 40;

pub(crate) struct SpawnedAppServer {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: Lines<BufReader<ChildStdout>>,
    pub(crate) stderr_tail: StderrTail,
    pub(crate) pid: global_types::Pid,
}

pub(crate) fn stderr_tail_context(tail: &StderrTail) -> String {
    let tail = stderr_tail_text(tail);
    if tail.is_empty() {
        String::new()
    } else {
        format!("; stderr tail: {tail}")
    }
}

pub(crate) fn stderr_tail_text(tail: &StderrTail) -> String {
    match tail.lock() {
        Ok(lines) => lines.iter().cloned().collect::<Vec<_>>().join("\n"),
        Err(_) => String::new(),
    }
}

#[tracing::instrument]
pub(crate) fn spawn_app_server() -> Result<SpawnedAppServer> {
    let codex = agent_runtime_core::resolve_codex_binary();
    let mut command = tokio::process::Command::new(codex.path());
    command
        .arg("app-server")
        .arg("--listen")
        .arg("stdio://")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    agent_runtime_core::apply_codex_binary_env(&mut command, &codex);
    let mut child = agent_runtime_core::spawn_isolated(command, ChildLifetime::KillOnDrop)
        .with_context(|| format!("spawn codex app-server at {}", codex.path().display()))?;
    let pid = global_types::Pid::new(child.id().context("codex app-server child had no pid")?);
    let stdin = child
        .stdin
        .take()
        .context("codex app-server stdin missing")?;
    let stdout = child
        .stdout
        .take()
        .context("codex app-server stdout missing")?;
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(drain_stderr(stderr, stderr_tail.clone()));
    }
    Ok(SpawnedAppServer {
        child,
        stdin,
        stdout: BufReader::new(stdout).lines(),
        stderr_tail,
        pid,
    })
}

#[tracing::instrument(skip(child), fields(reason))]
pub(crate) async fn cleanup_child_process(mut child: Child, reason: &'static str) {
    if let Some(pid) = child.id().map(global_types::Pid::new) {
        if let Err(e) = agent_runtime_core::kill_process(pid).await {
            tracing::warn!(module = "codex_app_server", pid = %pid, reason, error = %e, "failed to kill Codex app-server process group");
        }
    } else if let Err(e) = child.start_kill() {
        tracing::debug!(module = "codex_app_server", reason, error = %e, "Codex app-server child already exited before kill");
    }
    if let Err(e) = child.wait().await {
        tracing::debug!(module = "codex_app_server", reason, error = %e, "failed to wait for Codex app-server child");
    }
}

#[tracing::instrument(skip(stderr, tail))]
async fn drain_stderr(stderr: ChildStderr, tail: StderrTail) {
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                match tail.lock() {
                    Ok(mut lines) => {
                        if lines.len() >= STDERR_TAIL_LINES {
                            lines.pop_front();
                        }
                        lines.push_back(line.clone());
                    }
                    Err(_) => {
                        tracing::warn!(module = "codex_app_server", "stderr tail mutex poisoned")
                    }
                }
                tracing::debug!(module = "codex_app_server", stderr = %line, "codex app-server stderr");
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(module = "codex_app_server", error = %e, "failed to read Codex app-server stderr");
                break;
            }
        }
    }
}
