//! Codex usage warm-up: one throwaway `codex exec` prompt on a pooled
//! credential.
//!
//! OpenAI's Codex rate limits are rolling windows that only start counting
//! from the first request. An idle pool credential therefore has no reset
//! clock running at all: its 5h/7d windows begin the moment real work lands
//! on it, which is the worst possible time. Firing a trivial prompt while a
//! credential sits at zero usage starts the clock early, so by the time the
//! credential is picked its windows are already part-way to reset.
//!
//! The prompt runs in a private temp `CODEX_HOME` holding only the picked
//! `auth.json` and a file-store `config.toml`: no symlinks into `~/.codex`,
//! no user config, no MCP servers, `--ephemeral` so nothing is persisted to
//! session history. Codex may rotate the OAuth tokens during the run, so the
//! caller MUST sync `rotated_auth_json` back into the credential row —
//! refresh tokens are single-use and an unsynced rotation strands the
//! account. Never runs `codex logout` (that revokes the session server-side).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout, Command};

use super::codex_login::kill_and_reap;

/// Prefix for temp `CODEX_HOME` directories used by warm-up runs. Distinct
/// from the login (`mando-codex-login-`) and per-pick (`mando-codex-home-`)
/// prefixes so their sweeps never touch each other's directories.
const WARMUP_HOME_PREFIX: &str = "mando-codex-warmup-";
const FILE_AUTH_CONFIG: &str = "cli_auth_credentials_store = \"file\"\n";
const STDERR_TAIL_LINES: usize = 20;
/// Bound on the whole `codex exec` run. A one-word reply normally returns in
/// well under a minute; the bound covers slow model routing and token refresh.
pub const WARMUP_TIMEOUT: Duration = Duration::from_secs(180);
/// The prompt sent on every warm-up. Kept trivial so the run costs as close
/// to nothing as the provider allows while still counting as a request.
pub const WARMUP_PROMPT: &str = "Reply with the single word OK.";
/// Model every warm-up runs on. Pinned to the cheapest current Codex model
/// rather than the captain workflow's worker model: the prompt only has to
/// count as a request, so token cost is the only thing that matters.
pub const WARMUP_MODEL: &str = "gpt-6-luna";

/// Result of a successful warm-up run.
#[derive(Debug)]
pub struct CodexWarmupOutcome {
    /// `auth.json` as Codex left it, when it differs from what was written
    /// (token rotation happened during the run). The caller must persist it.
    pub rotated_auth_json: Option<String>,
    /// The model's final message, when Codex wrote one.
    pub last_message: Option<String>,
    pub elapsed: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexWarmupError {
    #[error("failed to prepare temp CODEX_HOME for warm-up: {0}")]
    HomeSetup(String),
    #[error("failed to spawn codex exec for warm-up: {0}")]
    Spawn(String),
    #[error("codex exec warm-up timed out after {0:?}")]
    Timeout(Duration),
    #[error("codex exec warm-up exited with {status}; stderr tail:\n{stderr_tail}")]
    NonZeroExit { status: String, stderr_tail: String },
    #[error("codex exec warm-up finished but auth.json is unreadable: {0}")]
    AuthJsonUnreadable(String),
}

/// Run one warm-up prompt with `auth_json` materialized into a private temp
/// `CODEX_HOME` on [`WARMUP_MODEL`]. The temp home is removed before
/// returning on every outcome.
#[tracing::instrument(skip_all)]
pub async fn run_codex_warmup(auth_json: &str) -> Result<CodexWarmupOutcome, CodexWarmupError> {
    let home =
        prepare_warmup_home(auth_json).map_err(|e| CodexWarmupError::HomeSetup(e.to_string()))?;
    let result = run_in_home(&home, auth_json).await;
    cleanup(&home);
    result
}

/// Create a fresh `0700` temp home holding `auth.json` (mode `0600`), a
/// file-store `config.toml`, and an empty working directory for the agent.
fn prepare_warmup_home(auth_json: &str) -> std::io::Result<PathBuf> {
    let dir = tempfile::Builder::new()
        .prefix(WARMUP_HOME_PREFIX)
        .tempdir()?
        .keep();
    let auth_path = dir.join("auth.json");
    std::fs::write(&auth_path, auth_json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::write(dir.join("config.toml"), FILE_AUTH_CONFIG)?;
    std::fs::create_dir(dir.join("workdir"))?;
    Ok(dir)
}

fn cleanup(dir: &Path) {
    global_infra::best_effort!(
        std::fs::remove_dir_all(dir),
        "cleanup temp codex warm-up home"
    );
}

async fn run_in_home(
    home: &Path,
    written_auth_json: &str,
) -> Result<CodexWarmupOutcome, CodexWarmupError> {
    let started = Instant::now();
    let last_message_path = home.join("last-message.txt");
    let codex = global_claude::resolve_codex_binary();
    let mut command = Command::new(codex.path());
    command
        .arg("exec")
        .arg("--ephemeral")
        .arg("--skip-git-repo-check")
        .arg("--ignore-rules")
        .arg("--color")
        .arg("never")
        .arg("--sandbox")
        .arg("read-only")
        .arg("-C")
        .arg(home.join("workdir"))
        .arg("-o")
        .arg(&last_message_path)
        .arg("-c")
        .arg("model_reasoning_effort=\"low\"")
        .arg("-m")
        .arg(WARMUP_MODEL)
        .arg(WARMUP_PROMPT)
        .env("CODEX_HOME", home)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in global_claude::DAEMON_ENV_STRIP {
        command.env_remove(key);
    }
    global_claude::apply_codex_binary_env(&mut command, &codex);
    #[cfg(unix)]
    unsafe {
        // Own process group so a timeout kill reaches the real codex-rs
        // binary behind the npm shim, not just the shim.
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command
        .spawn()
        .map_err(|e| CodexWarmupError::Spawn(e.to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CodexWarmupError::Spawn("codex exec stderr pipe missing".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CodexWarmupError::Spawn("codex exec stdout pipe missing".to_string()))?;
    tokio::spawn(drain_ignored(stdout));
    let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let stderr_task = tokio::spawn(drain_stderr(stderr, stderr_tail.clone()));

    let status = match tokio::time::timeout(WARMUP_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            stderr_task.abort();
            return Err(CodexWarmupError::Spawn(format!(
                "failed waiting for codex exec child: {e}"
            )));
        }
        Err(_) => {
            kill_and_reap(&mut child).await;
            stderr_task.abort();
            return Err(CodexWarmupError::Timeout(WARMUP_TIMEOUT));
        }
    };
    global_infra::best_effort!(stderr_task.await, "join codex warm-up stderr drain task");
    if !status.success() {
        return Err(CodexWarmupError::NonZeroExit {
            status: status.to_string(),
            stderr_tail: stderr_tail_text(&stderr_tail),
        });
    }

    let current_auth_json = tokio::fs::read_to_string(home.join("auth.json"))
        .await
        .map_err(|e| CodexWarmupError::AuthJsonUnreadable(e.to_string()))?;
    let rotated_auth_json = (current_auth_json != written_auth_json).then_some(current_auth_json);
    let last_message = tokio::fs::read_to_string(&last_message_path)
        .await
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    Ok(CodexWarmupOutcome {
        rotated_auth_json,
        last_message,
        elapsed: started.elapsed(),
    })
}

fn stderr_tail_text(tail: &Mutex<VecDeque<String>>) -> String {
    match tail.lock() {
        Ok(lines) => lines.iter().cloned().collect::<Vec<_>>().join("\n"),
        Err(_) => String::new(),
    }
}

async fn drain_stderr(stderr: ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match tail.lock() {
                Ok(mut tail) => {
                    if tail.len() >= STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
                Err(_) => tracing::warn!(
                    module = "settings-io-codex_warmup",
                    "stderr tail mutex poisoned"
                ),
            },
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(
                    module = "settings-io-codex_warmup",
                    error = %e,
                    "failed to read codex exec stderr"
                );
                break;
            }
        }
    }
}

/// Keep the stdout pipe drained so a chatty run can never stall the child;
/// the final message is read from the `-o` file instead.
async fn drain_ignored(stdout: ChildStdout) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(_line)) = lines.next_line().await {}
}
