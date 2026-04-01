//! Process lifecycle management — spawn, monitor, kill.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::CcConfig;

/// Spawn a claude subprocess with stream-json input/output.
///
/// Returns `(child, pid, stream_path, stderr_path)`.
/// Stdin is piped for bidirectional communication.
/// Stdout is piped for reading messages.
/// Stderr goes to a file.
pub(crate) async fn spawn_process(
    config: &CcConfig,
    session_id: &str,
) -> Result<(tokio::process::Child, u32, PathBuf, PathBuf)> {
    let claude = crate::resolve_claude_binary();
    let stream_dir = mando_config::cc_streams_dir();
    std::fs::create_dir_all(&stream_dir)?;

    let stream_path = stream_dir.join(format!("{session_id}.jsonl"));
    let stderr_path = stream_dir.join(format!("{session_id}.stderr"));

    // Stream file: append for resume, create for new.
    let _stream_file = if config.resume_session_id.is_some() {
        std::fs::File::options()
            .create(true)
            .append(true)
            .open(&stream_path)
            .with_context(|| format!("open stream log: {}", stream_path.display()))?
    } else {
        std::fs::File::create(&stream_path)
            .with_context(|| format!("create stream log: {}", stream_path.display()))?
    };
    let stderr_file = std::fs::File::options()
        .create(true)
        .append(true)
        .open(&stderr_path)
        .with_context(|| format!("open stderr log: {}", stderr_path.display()))?;

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
        .stderr(std::process::Stdio::from(stderr_file));

    // Tee stdout to stream file via a background task (handled by caller).
    // For now, stdout is piped directly — caller reads from it and writes to file.

    // Environment.
    cmd.env("CLAUDE_CODE_EXIT_AFTER_STOP_DELAY", "5000");
    // Enable CC's streaming watchdog — aborts hung API connections (not tool
    // execution). Without this, a silently-dropped SSE connection hangs forever.
    cmd.env("CLAUDE_ENABLE_STREAM_WATCHDOG", "1");
    cmd.env_remove("CLAUDECODE");
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

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn claude at {:?}: {}", claude, e))?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("process exited before PID read"))?;

    Ok((child, pid, stream_path, stderr_path))
}

/// Spawn a detached worker process — fire-and-forget with stdout to file.
///
/// This is the bridge pattern for long-lived workers. The worker runs autonomously;
/// captain monitors it via the stream file and PID. Stdout/stderr go to files.
///
/// Returns `(pid, stream_path)`.
///
/// NOTE: This uses `-p` (not stream-json input) because the worker is detached.
/// Full stream-json input for workers (nudges via stdin) is planned but requires
/// holding process handles across captain ticks.
pub async fn spawn_detached(
    config: &CcConfig,
    prompt: &str,
    session_id: &str,
) -> Result<(u32, PathBuf)> {
    let claude = crate::resolve_claude_binary();
    let stream_dir = mando_config::cc_streams_dir();
    std::fs::create_dir_all(&stream_dir)?;

    let stream_path = stream_dir.join(format!("{session_id}.jsonl"));
    let stderr_path = stream_dir.join(format!("{session_id}.stderr"));

    let stream_file = if config.resume_session_id.is_some() {
        std::fs::File::options()
            .create(true)
            .append(true)
            .open(&stream_path)
            .with_context(|| format!("open stream: {}", stream_path.display()))?
    } else {
        std::fs::File::create(&stream_path)
            .with_context(|| format!("create stream: {}", stream_path.display()))?
    };
    let stderr_file = std::fs::File::options()
        .create(true)
        .append(true)
        .open(&stderr_path)
        .with_context(|| format!("open stderr: {}", stderr_path.display()))?;

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
    cmd.env("CLAUDE_ENABLE_STREAM_WATCHDOG", "1");
    cmd.env_remove("CLAUDECODE");
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

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn detached claude at {:?}: {}", claude, e))?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("process exited before PID read"))?;

    Ok((pid, stream_path))
}

/// Kill a process: SIGTERM → poll 5s → SIGKILL.
pub async fn kill_process(pid: u32) -> Result<()> {
    if pid == 0 {
        tracing::warn!(
            module = "mando-cc",
            "kill_process called with pid=0, skipping"
        );
        return Ok(());
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }

    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if !is_process_alive(pid) {
            return Ok(());
        }
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    Ok(())
}

/// Check if a process is alive.
pub fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Get CPU time in seconds for a process via `ps -o cputime=`.
pub async fn get_cpu_time(pid: u32) -> Result<f64> {
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
        assert!(!is_process_alive(0));
    }
}
