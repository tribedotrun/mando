//! Process lifecycle management — spawn, monitor, kill.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::CcConfig;
use crate::error::CcError;

/// Daemon env vars stripped from worker processes so they don't inherit
/// state from the parent context. Two reasons covered here:
///
/// - mode state that silently changes mando-dev behavior (e.g. MANDO_PROD_MODE
///   turning `start dev` into prod-local);
/// - terminal-session attribution (MANDO_TERMINAL_ID / MANDO_TERMINAL_CWD).
///   If the daemon was launched from inside a mando-spawned terminal, these
///   leak into every captain CC subprocess. The CC SessionStart hook then
///   POSTs the captain session's id to /api/terminal/{leaked_id}/cc-session,
///   overwriting that terminal's cc_session_id with a conversation file that
///   lives under the captain's cwd hash — a different project hash than the
///   terminal's home cwd. After a daemon restart, auto-resume launches
///   `claude --resume <id>` from the terminal's cwd, CC computes the wrong
///   project hash, and exits with "No conversation found".
pub const DAEMON_ENV_STRIP: &[&str] = &[
    "MANDO_PROD_MODE",
    "MANDO_APP_MODE",
    "MANDO_SANDBOX",
    "MANDO_ELECTRON_BIN",
    "MANDO_ELECTRON_ENTRYPOINT",
    "MANDO_ELECTRON_INSPECT_PORT",
    "MANDO_ELECTRON_CDP_PORT",
    "MANDO_EXTERNAL_GATEWAY",
    "MANDO_TERMINAL_ID",
    "MANDO_TERMINAL_CWD",
];

/// Spawn a claude subprocess with stream-json input/output.
///
/// Returns `(child, pid, stream_path, stderr_path)`.
/// Stdin is piped for bidirectional communication.
/// Stdout is piped for reading messages.
/// Stderr goes to a file.
/// Spawn a Claude Code process attached to the parent (stdin/stdout piped for
/// interactive streaming). Returns the child handle, its `Pid`, and the paths
/// to the stream and stderr log files.
pub(crate) async fn spawn_process(
    config: &CcConfig,
    session_id: &str,
) -> Result<(tokio::process::Child, global_types::Pid, PathBuf, PathBuf), CcError> {
    let claude = crate::resolve_claude_binary();
    let stream_dir = global_infra::paths::cc_streams_dir();
    tokio::fs::create_dir_all(&stream_dir).await?;

    let stream_path = stream_dir.join(format!("{session_id}.jsonl"));
    let stderr_path = stream_dir.join(format!("{session_id}.stderr"));

    // Stream file: append for resume, create for new.
    // `std::process::Stdio::from` needs a blocking `std::fs::File`, so these
    // opens must stay blocking — wrap them in spawn_blocking to avoid stalling
    // the async runtime.
    let stream_path_clone = stream_path.clone();
    let stderr_path_clone = stderr_path.clone();
    let resume = config.resume_session_id.is_some();
    let (_stream_file, stderr_file) = tokio::task::spawn_blocking(move || -> Result<_> {
        let stream_file = if resume {
            std::fs::File::options()
                .create(true)
                .append(true)
                .open(&stream_path_clone)
                .with_context(|| format!("open stream log: {}", stream_path_clone.display()))?
        } else {
            std::fs::File::create(&stream_path_clone)
                .with_context(|| format!("create stream log: {}", stream_path_clone.display()))?
        };
        let stderr_file = std::fs::File::options()
            .create(true)
            .append(true)
            .open(&stderr_path_clone)
            .with_context(|| format!("open stderr log: {}", stderr_path_clone.display()))?;
        Ok((stream_file, stderr_file))
    })
    .await
    .map_err(|e| CcError::Other(anyhow::Error::new(e)))?
    .map_err(CcError::Other)?;

    let mut args = config.to_cli_args();

    // Ensure session_id is always passed to the CLI — if neither resume nor
    // session_id was set in the config, add --session-id with the generated ID.
    if config.resume_session_id.is_none() && config.session_id.is_none() {
        args.push("--session-id".into());
        args.push(session_id.into());
    }

    let mut cmd = tokio::process::Command::new(&claude);
    cmd.args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::from(stderr_file))
        // Interactive sessions: kill the subprocess when the `Child` handle is
        // dropped. Prevents Claude subprocess leaks when `CcSession` errors out
        // before its explicit `close()` path runs.
        .kill_on_drop(true);

    // Tee stdout to stream file via a background task (handled by caller).
    // For now, stdout is piped directly — caller reads from it and writes to file.

    // Environment.
    cmd.env("CLAUDE_CODE_EXIT_AFTER_STOP_DELAY", "5000");
    cmd.env_remove("CLAUDECODE");
    // Strip daemon-specific env vars so workers don't inherit state that
    // causes mando-dev commands to silently change mode (e.g. start dev
    // becoming prod-local when MANDO_PROD_MODE is set).
    for key in DAEMON_ENV_STRIP {
        cmd.env_remove(key);
    }
    if config.caller.starts_with("scout-") {
        cmd.env("DISABLE_LANG_GUARD", "1");
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    // Working directory.
    if !config.cwd.as_os_str().is_empty() {
        cmd.current_dir(&config.cwd);
    }

    // Process group independence.
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().map_err(|e| CcError::SpawnFailed {
        binary: claude.clone(),
        source: e,
    })?;
    let pid = global_types::Pid::new(child.id().ok_or(CcError::StreamClosed)?);

    Ok((child, pid, stream_path, stderr_path))
}

/// Spawn a detached worker process — fire-and-forget with stdout to file.
///
/// This is the bridge pattern for long-lived workers. The worker runs autonomously;
/// captain monitors it via the stream file and PID. Stdout/stderr go to files.
///
/// Returns `(child, pid, stream_path)`. The caller owns the `Child` handle and
/// should `.wait()` on it to detect process exit (e.g. via a background watcher).
///
/// NOTE: This uses `-p` (not stream-json input) because the worker is detached.
pub async fn spawn_detached(
    config: &CcConfig,
    prompt: &str,
    session_id: &str,
) -> Result<(tokio::process::Child, global_types::Pid, PathBuf), CcError> {
    let claude = crate::resolve_claude_binary();
    let stream_dir = global_infra::paths::cc_streams_dir();
    tokio::fs::create_dir_all(&stream_dir).await?;

    let stream_path = stream_dir.join(format!("{session_id}.jsonl"));
    let stderr_path = stream_dir.join(format!("{session_id}.stderr"));

    // `std::process::Stdio::from` needs a blocking `std::fs::File`, so these
    // opens must stay blocking — wrap them in spawn_blocking to avoid stalling
    // the async runtime.
    let stream_path_clone = stream_path.clone();
    let stderr_path_clone = stderr_path.clone();
    let resume = config.resume_session_id.is_some();
    let (stream_file, stderr_file) = tokio::task::spawn_blocking(move || -> Result<_> {
        let stream_file = if resume {
            std::fs::File::options()
                .create(true)
                .append(true)
                .open(&stream_path_clone)
                .with_context(|| format!("open stream: {}", stream_path_clone.display()))?
        } else {
            std::fs::File::create(&stream_path_clone)
                .with_context(|| format!("create stream: {}", stream_path_clone.display()))?
        };
        let stderr_file = std::fs::File::options()
            .create(true)
            .append(true)
            .open(&stderr_path_clone)
            .with_context(|| format!("open stderr: {}", stderr_path_clone.display()))?;
        Ok((stream_file, stderr_file))
    })
    .await
    .map_err(|e| CcError::Other(anyhow::Error::new(e)))?
    .map_err(CcError::Other)?;

    // Build args — reuse to_cli_args, then prepend -p and fix session-id for
    // detached mode (prompt via CLI flag, not stdin).
    let mut args = config.to_cli_args();

    // Replace --input-format stream-json with -p (detached workers get prompt
    // via CLI flag, not stdin).
    if let Some(pos) = args.iter().position(|a| a == "--input-format") {
        // Remove --input-format and its value
        args.remove(pos); // --input-format
        if pos < args.len() {
            args.remove(pos); // stream-json
        }
    }
    args.insert(0, prompt.into());
    args.insert(0, "-p".into());

    // For detached workers without an explicit resume, always assign the
    // provided session-id so we can track the stream file.
    if config.resume_session_id.is_none() {
        // to_cli_args may have set --session-id from config; override it.
        if let Some(pos) = args.iter().position(|a| a == "--session-id") {
            if pos + 1 < args.len() {
                args[pos + 1] = session_id.into();
            }
        } else {
            args.push("--session-id".into());
            args.push(session_id.into());
        }
    }

    let mut cmd = tokio::process::Command::new(&claude);
    cmd.args(&args)
        .stdout(std::process::Stdio::from(stream_file))
        .stderr(std::process::Stdio::from(stderr_file));

    // Environment.
    cmd.env("CLAUDE_CODE_EXIT_AFTER_STOP_DELAY", "5000");
    cmd.env_remove("CLAUDECODE");
    for key in DAEMON_ENV_STRIP {
        cmd.env_remove(key);
    }
    if config.caller.starts_with("scout-") {
        cmd.env("DISABLE_LANG_GUARD", "1");
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    if !config.cwd.as_os_str().is_empty() {
        cmd.current_dir(&config.cwd);
    }

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().map_err(|e| CcError::SpawnFailed {
        binary: claude.clone(),
        source: e,
    })?;
    let pid = global_types::Pid::new(child.id().ok_or(CcError::StreamClosed)?);

    Ok((child, pid, stream_path))
}

/// Kill a process: SIGTERM → poll 5s → SIGKILL.
///
/// Detached workers are owned by captain via a stream file + PID, not a
/// `Child` handle, so we can't `.wait()` on them. We poll `is_process_alive`
/// with tokio::time::sleep inside a bounded `tokio::time::timeout` instead of
/// a hand-rolled loop counter.
pub async fn kill_process(pid: global_types::Pid) -> Result<()> {
    if pid.as_u32() == 0 {
        tracing::warn!(
            module = "mando-cc",
            "kill_process called with pid=0, skipping"
        );
        return Ok(());
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-pid.as_i32(), libc::SIGTERM);
    }

    let wait_exit = async {
        while is_process_alive(pid) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    };
    if tokio::time::timeout(std::time::Duration::from_secs(5), wait_exit)
        .await
        .is_ok()
    {
        return Ok(());
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-pid.as_i32(), libc::SIGKILL);
    }
    Ok(())
}

/// Check if a process is alive.
pub fn is_process_alive(pid: global_types::Pid) -> bool {
    if pid.as_u32() == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid.as_i32(), 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Get CPU time in seconds for a process via `ps -o cputime=`.
pub async fn get_cpu_time(pid: global_types::Pid) -> Result<f64> {
    let output = tokio::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("cputime=")
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_cputime(&text)
}

fn parse_cputime(s: &str) -> Result<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse()?;
            let m: f64 = parts[1].parse()?;
            let s: f64 = parts[2].parse()?;
            Ok(h * 3600.0 + m * 60.0 + s)
        }
        2 => {
            let m: f64 = parts[0].parse()?;
            let s: f64 = parts[1].parse()?;
            Ok(m * 60.0 + s)
        }
        _ => anyhow::bail!("invalid cputime format: {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cputime_hhmmss() {
        assert!((parse_cputime("01:30:45").unwrap() - 5445.0).abs() < 0.1);
    }

    #[test]
    fn parse_cputime_mmss() {
        assert!((parse_cputime("05:30").unwrap() - 330.0).abs() < 0.1);
    }

    #[test]
    fn pid_zero_not_alive() {
        assert!(!is_process_alive(global_types::Pid::new(0)));
    }

    #[test]
    fn daemon_env_strip_includes_terminal_attribution_keys() {
        // Captain CC subprocesses must NOT inherit MANDO_TERMINAL_ID /
        // MANDO_TERMINAL_CWD from the daemon process. If the daemon was
        // launched from inside a mando-spawned terminal, these would leak
        // and the SessionStart hook would attribute the captain session to
        // that terminal — breaking subsequent `claude --resume` because the
        // conversation file lives under the captain's cwd hash, not the
        // terminal's home cwd.
        assert!(
            DAEMON_ENV_STRIP.contains(&"MANDO_TERMINAL_ID"),
            "MANDO_TERMINAL_ID must be stripped from CC subprocess env"
        );
        assert!(
            DAEMON_ENV_STRIP.contains(&"MANDO_TERMINAL_CWD"),
            "MANDO_TERMINAL_CWD must be stripped from CC subprocess env"
        );
    }
}
