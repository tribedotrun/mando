//! Spawn `codex login`, capture the browser OAuth URL from stderr, and
//! collect the resulting `auth.json` once the flow finishes.
//!
//! `codex login` binds a local callback server (127.0.0.1:1455, fallback
//! 1457), opens the user's browser itself, and blocks until the OAuth
//! round-trip completes — it has no internal timeout, so every caller MUST
//! race the wait against [`LOGIN_TIMEOUT`] and a cancellation token. All
//! process output lands on stderr; stdout is unused.
//!
//! Never runs `codex logout` against the temp `CODEX_HOME` — that would
//! revoke the just-captured session server-side.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio_util::sync::CancellationToken;

/// Prefix for temp `CODEX_HOME` directories materialized for a browser
/// login attempt. Distinct from `mando-codex-home-` (the per-pick prefix
/// used by `rust/cli/src/credentials_codex_pick.rs`) so the two sweeps
/// never touch each other's directories.
const LOGIN_HOME_PREFIX: &str = "mando-codex-login-";
const STDERR_TAIL_LINES: usize = 20;
const STALE_HOME_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FILE_AUTH_CONFIG: &str = "cli_auth_credentials_store = \"file\"\n";

/// External timeout for the browser OAuth round-trip. `codex login` itself
/// never times out on its own.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

static AUTH_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"https://\S*/oauth/authorize\?\S+") {
        Ok(re) => re,
        Err(e) => global_infra::unrecoverable!("AUTH_URL_RE compilation failed", e),
    });

/// Successful `codex login` outcome: the raw `auth.json` contents.
#[derive(Debug)]
pub struct CodexLoginCapture {
    pub auth_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexLoginError {
    #[error("failed to prepare temp CODEX_HOME: {0}")]
    HomeSetup(String),
    #[error("failed to spawn codex login: {0}")]
    Spawn(String),
    #[error("codex login timed out after {0:?} waiting for the browser sign-in to complete")]
    Timeout(Duration),
    #[error("codex login was cancelled")]
    Cancelled,
    #[error("codex login exited with {status}; stderr tail:\n{stderr_tail}")]
    NonZeroExit { status: String, stderr_tail: String },
    #[error("codex login succeeded but auth.json is missing or unreadable: {0}")]
    AuthJsonUnreadable(String),
}

/// Extract the browser OAuth authorize URL from one `codex login` stderr
/// line, if present.
pub fn extract_auth_url(line: &str) -> Option<String> {
    AUTH_URL_RE.find(line).map(|m| m.as_str().to_string())
}

/// True when `path` is a Mando-managed `codex login` temp home under the
/// system temp folder.
fn is_login_home(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(LOGIN_HOME_PREFIX))
        && path.starts_with(std::env::temp_dir())
}

/// True when a managed login home last modified at `modified` is older
/// than [`STALE_HOME_MAX_AGE`] as of `now`.
fn is_stale(modified: std::time::SystemTime, now: std::time::SystemTime) -> bool {
    match now.duration_since(modified) {
        Ok(age) => age > STALE_HOME_MAX_AGE,
        Err(_) => false,
    }
}

/// Sweep the system temp folder for abandoned `codex login` temp homes
/// (e.g. left behind by a daemon crash mid-flow) and remove any older than
/// 24h. Never touches a live/fresh flow's home. Best-effort; call this
/// before starting a new flow.
pub fn prune_stale_login_homes() {
    let temp = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&temp) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_login_home(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if !is_stale(modified, now) {
            continue;
        }
        global_infra::best_effort!(
            std::fs::remove_dir_all(&path),
            "prune stale codex login temp home"
        );
    }
}

/// Create a fresh temp `CODEX_HOME` and write `config.toml` so `codex
/// login` always writes `auth.json` as a plain file (defends against
/// keyring config inherited from the ambient environment).
///
/// The directory is created atomically with `0700` permissions and a
/// random suffix via `tempfile` — a predictable name under the shared temp
/// dir could be pre-created by another local user before OAuth material
/// lands in it. `keep()` releases it from `TempDir`'s drop-guard: cleanup
/// stays manual via [`cleanup`] and [`prune_stale_login_homes`], which
/// still match because the prefix is preserved.
fn prepare_login_home() -> std::io::Result<PathBuf> {
    let dir = tempfile::Builder::new()
        .prefix(LOGIN_HOME_PREFIX)
        .tempdir()?
        .keep();
    std::fs::write(dir.join("config.toml"), FILE_AUTH_CONFIG)?;
    Ok(dir)
}

/// Best-effort removal of a temp login home. Never runs `codex logout`.
fn cleanup(dir: &Path) {
    global_infra::best_effort!(
        std::fs::remove_dir_all(dir),
        "cleanup temp codex login home"
    );
}

fn stderr_tail_text(tail: &Mutex<VecDeque<String>>) -> String {
    match tail.lock() {
        Ok(lines) => lines.iter().cloned().collect::<Vec<_>>().join("\n"),
        Err(_) => String::new(),
    }
}

/// Spawn `codex login` in a fresh temp `CODEX_HOME`, wait for the browser
/// OAuth flow to complete (bounded by [`LOGIN_TIMEOUT`] and `cancel`), and
/// return the resulting `auth.json`. `on_auth_url` fires at most once, as
/// soon as the callback URL appears in stderr, so the caller can publish it
/// to UI-visible state while the child is still running. The temp home is
/// always cleaned up before returning, on every outcome.
pub async fn run_codex_login(
    on_auth_url: impl Fn(String) + Send + Sync + 'static,
    cancel: CancellationToken,
) -> Result<CodexLoginCapture, CodexLoginError> {
    let home = prepare_login_home().map_err(|e| CodexLoginError::HomeSetup(e.to_string()))?;
    let result = run_in_home(&home, on_auth_url, cancel).await;
    cleanup(&home);
    result
}

enum WaitOutcome {
    Exited(std::process::ExitStatus),
    Cancelled,
    TimedOut,
}

async fn run_in_home(
    home: &Path,
    on_auth_url: impl Fn(String) + Send + Sync + 'static,
    cancel: CancellationToken,
) -> Result<CodexLoginCapture, CodexLoginError> {
    let codex = global_claude::resolve_codex_binary();
    let mut command = Command::new(codex.path());
    command
        .arg("login")
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
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command
        .spawn()
        .map_err(|e| CodexLoginError::Spawn(e.to_string()))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CodexLoginError::Spawn("codex login stderr pipe missing".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CodexLoginError::Spawn("codex login stdout pipe missing".to_string()))?;
    tokio::spawn(drain_ignored(stdout));

    let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let stderr_task = tokio::spawn(drain_stderr(stderr, stderr_tail.clone(), on_auth_url));

    let outcome = tokio::select! {
        _ = cancel.cancelled() => Ok(WaitOutcome::Cancelled),
        res = tokio::time::timeout(LOGIN_TIMEOUT, child.wait()) => match res {
            Ok(Ok(status)) => Ok(WaitOutcome::Exited(status)),
            Ok(Err(e)) => Err(CodexLoginError::Spawn(format!(
                "failed waiting for codex login child: {e}"
            ))),
            Err(_) => Ok(WaitOutcome::TimedOut),
        },
    };

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(e) => {
            stderr_task.abort();
            return Err(e);
        }
    };

    finish(&mut child, home, outcome, stderr_task, stderr_tail).await
}

async fn finish(
    child: &mut Child,
    home: &Path,
    outcome: WaitOutcome,
    stderr_task: tokio::task::JoinHandle<()>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
) -> Result<CodexLoginCapture, CodexLoginError> {
    match outcome {
        WaitOutcome::Exited(status) => {
            global_infra::best_effort!(stderr_task.await, "join codex login stderr drain task");
            if status.success() {
                match tokio::fs::read_to_string(home.join("auth.json")).await {
                    Ok(auth_json) => Ok(CodexLoginCapture { auth_json }),
                    Err(e) => Err(CodexLoginError::AuthJsonUnreadable(e.to_string())),
                }
            } else {
                Err(CodexLoginError::NonZeroExit {
                    status: status.to_string(),
                    stderr_tail: stderr_tail_text(&stderr_tail),
                })
            }
        }
        WaitOutcome::Cancelled => {
            kill_and_reap(child).await;
            stderr_task.abort();
            Err(CodexLoginError::Cancelled)
        }
        WaitOutcome::TimedOut => {
            kill_and_reap(child).await;
            stderr_task.abort();
            Err(CodexLoginError::Timeout(LOGIN_TIMEOUT))
        }
    }
}

/// Kill the login child and reap it. On unix the SIGKILL targets the whole
/// process group, not just the direct child: the resolved `codex` binary is
/// usually the npm shim, a Node wrapper that spawns the real codex-rs
/// binary as a grandchild, and SIGKILL to the shim alone does not forward —
/// the real `codex login` would survive as an orphan holding the 1455
/// callback port forever (its browser flow never times out). The spawn's
/// `pre_exec` calls `setsid()`, so the child is its own process-group
/// leader and `kill(-pid, SIGKILL)` reaches the grandchildren too.
async fn kill_and_reap(child: &mut Child) {
    #[cfg(unix)]
    match child.id() {
        Some(pid) => {
            let group_kill = if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            };
            global_infra::best_effort!(group_kill, "kill codex login process group");
        }
        // Child already reaped (no pid); fall back to the direct-kill path,
        // which degrades to a no-op error on an exited child.
        None => global_infra::best_effort!(child.start_kill(), "kill codex login child"),
    }
    #[cfg(not(unix))]
    global_infra::best_effort!(child.start_kill(), "kill codex login child");
    global_infra::best_effort!(child.wait().await, "reap killed codex login child");
}

/// Read `codex login` stderr to EOF, invoking `on_auth_url` at most once
/// (on the first line that matches the OAuth authorize URL pattern) and
/// keeping a bounded tail of every line for error reporting.
async fn drain_stderr(
    stderr: ChildStderr,
    tail: Arc<Mutex<VecDeque<String>>>,
    on_auth_url: impl Fn(String) + Send + Sync + 'static,
) {
    let mut found_url = false;
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if !found_url {
                    if let Some(url) = extract_auth_url(&line) {
                        found_url = true;
                        on_auth_url(url);
                    }
                }
                match tail.lock() {
                    Ok(mut tail) => {
                        if tail.len() >= STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                        tail.push_back(line);
                    }
                    Err(_) => tracing::warn!(
                        module = "settings-io-codex_login",
                        "stderr tail mutex poisoned"
                    ),
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(module = "settings-io-codex_login", error = %e, "failed to read codex login stderr");
                break;
            }
        }
    }
}

/// Discard `codex login` stdout. The CLI writes nothing there, but the pipe
/// still needs a reader so an unexpected write can never stall the child.
async fn drain_ignored(stdout: ChildStdout) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(_line)) = lines.next_line().await {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn unique_temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mando-codex-login-test-{label}-{}-{}",
            std::process::id(),
            global_infra::uuid::Uuid::v4()
        ))
    }

    #[cfg(unix)]
    fn write_executable_script(contents: &str) -> PathBuf {
        let path = unique_temp_path("bin");
        std::fs::write(&path, contents).expect("write test script");
        let mut perms = std::fs::metadata(&path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod script");
        path
    }

    // ── pure helpers ──────────────────────────────────────────────────

    #[test]
    fn extract_auth_url_matches_authorize_link() {
        let line = "https://auth.openai.com/oauth/authorize?client_id=abc&redirect_uri=http://localhost:1455/callback";
        assert_eq!(extract_auth_url(line).as_deref(), Some(line));
    }

    #[test]
    fn extract_auth_url_matches_embedded_in_surrounding_text() {
        let line = "  navigate to https://auth.openai.com/oauth/authorize?client_id=abc here";
        assert_eq!(
            extract_auth_url(line).as_deref(),
            Some("https://auth.openai.com/oauth/authorize?client_id=abc")
        );
    }

    #[test]
    fn extract_auth_url_ignores_unrelated_lines() {
        assert_eq!(
            extract_auth_url("Starting local login server on http://localhost:1455."),
            None
        );
    }

    #[test]
    fn file_auth_config_pins_file_store() {
        assert_eq!(FILE_AUTH_CONFIG, "cli_auth_credentials_store = \"file\"\n");
    }

    #[test]
    fn is_stale_flags_dirs_older_than_24h() {
        let now = std::time::SystemTime::now();
        let old = now - Duration::from_secs(25 * 60 * 60);
        let recent = now - Duration::from_secs(60);
        assert!(is_stale(old, now));
        assert!(!is_stale(recent, now));
    }

    #[test]
    fn is_login_home_requires_prefix_and_temp_dir() {
        let managed = std::env::temp_dir().join("mando-codex-login-1-2");
        let unmanaged = std::env::temp_dir().join("something-else");
        assert!(is_login_home(&managed));
        assert!(!is_login_home(&unmanaged));
    }

    // ── integration-style tests against a fake `codex` binary ──────────

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_login_captures_auth_json_and_url_then_cleans_up() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let marker = unique_temp_path("marker-success");
        let script = format!(
            "#!/bin/sh\n\
             echo \"$CODEX_HOME\" > {marker}\n\
             echo 'If your browser did not open, navigate to this URL to authenticate:' 1>&2\n\
             echo '' 1>&2\n\
             echo 'https://auth.openai.com/oauth/authorize?client_id=abc&redirect_uri=http://localhost:1455/callback' 1>&2\n\
             cat > \"$CODEX_HOME/auth.json\" <<'JSON'\n\
             {{\"auth_mode\":\"chatgpt\",\"tokens\":{{\"access_token\":\"AT\",\"refresh_token\":\"RT\",\"account_id\":\"acct-test\"}}}}\n\
             JSON\n\
             echo 'Successfully logged in' 1>&2\n\
             exit 0\n",
            marker = marker.display()
        );
        let script_path = write_executable_script(&script);
        let _guard = global_infra::EnvVarGuard::set("MANDO_CODEX_BIN", &script_path);

        let captured_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_url_cb = captured_url.clone();
        let result = run_codex_login(
            move |url| {
                if let Ok(mut guard) = captured_url_cb.lock() {
                    *guard = Some(url);
                }
            },
            CancellationToken::new(),
        )
        .await;

        let capture = result.expect("login should succeed");
        assert!(capture.auth_json.contains("acct-test"));
        assert_eq!(
            captured_url.lock().expect("lock").as_deref(),
            Some(
                "https://auth.openai.com/oauth/authorize?client_id=abc&redirect_uri=http://localhost:1455/callback"
            )
        );

        let home = std::fs::read_to_string(&marker).expect("marker written");
        let home = home.trim();
        assert!(
            !Path::new(home).exists(),
            "temp CODEX_HOME should be cleaned up after success"
        );

        global_infra::best_effort!(std::fs::remove_file(&script_path), "test cleanup");
        global_infra::best_effort!(std::fs::remove_file(&marker), "test cleanup");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nonzero_exit_returns_error_with_stderr_tail() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let marker = unique_temp_path("marker-fail");
        let script = format!(
            "#!/bin/sh\n\
             echo \"$CODEX_HOME\" > {marker}\n\
             echo 'Error logging in: Port 1455 is already in use' 1>&2\n\
             exit 1\n",
            marker = marker.display()
        );
        let script_path = write_executable_script(&script);
        let _guard = global_infra::EnvVarGuard::set("MANDO_CODEX_BIN", &script_path);

        let result = run_codex_login(|_url| {}, CancellationToken::new()).await;
        match result {
            Err(CodexLoginError::NonZeroExit { stderr_tail, .. }) => {
                assert!(stderr_tail.contains("Port 1455 is already in use"));
            }
            other => panic!("expected NonZeroExit, got {other:?}"),
        }

        let home = std::fs::read_to_string(&marker).expect("marker written");
        assert!(!Path::new(home.trim()).exists());

        global_infra::best_effort!(std::fs::remove_file(&script_path), "test cleanup");
        global_infra::best_effort!(std::fs::remove_file(&marker), "test cleanup");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_kills_child_promptly_and_cleans_up() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let marker = unique_temp_path("marker-cancel");
        let script = format!(
            "#!/bin/sh\necho \"$CODEX_HOME\" > {marker}\nsleep 30\n",
            marker = marker.display()
        );
        let script_path = write_executable_script(&script);
        let _guard = global_infra::EnvVarGuard::set("MANDO_CODEX_BIN", &script_path);

        let cancel = CancellationToken::new();
        let login_task = tokio::spawn(run_codex_login(|_url| {}, cancel.clone()));

        // Wait until the child has actually started (marker written) before
        // cancelling, so this proves cancellation interrupts a live process
        // rather than racing process-spawn latency with a fixed sleep.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "codex login child did not start within 5s"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let started = std::time::Instant::now();
        cancel.cancel();
        let result = login_task.await.expect("login task join");
        assert!(matches!(result, Err(CodexLoginError::Cancelled)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "cancellation should kill the child promptly instead of waiting out the sleep 30"
        );

        let home = std::fs::read_to_string(&marker).expect("marker written before sleep");
        assert!(!Path::new(home.trim()).exists());

        global_infra::best_effort!(std::fs::remove_file(&script_path), "test cleanup");
        global_infra::best_effort!(std::fs::remove_file(&marker), "test cleanup");
    }
}
