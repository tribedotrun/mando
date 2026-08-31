//! Process lifecycle management — spawn, monitor, kill.

use std::path::PathBuf;

use agent_runtime_core::ChildLifetime;
use anyhow::{Context, Result};

use crate::config::CcConfig;
use crate::error::CcError;

/// Spawn a Claude Code process attached to the parent (stdin/stdout piped for
/// interactive streaming). Returns the child handle, its `Pid`, the stream
/// path, and the already-open stream file used to tee stdout. Stderr goes
/// directly to its log file.
pub(crate) async fn spawn_process(
    config: &CcConfig,
    session_id: &str,
) -> Result<
    (
        tokio::process::Child,
        global_types::Pid,
        PathBuf,
        std::fs::File,
    ),
    CcError,
> {
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
    let (stream_file, stderr_file) = tokio::task::spawn_blocking(move || -> Result<_> {
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
        .stderr(std::process::Stdio::from(stderr_file));

    // Stdout stays piped; the caller reads it and tees each line into stream_file.

    // Environment.
    cmd.env("CLAUDE_CODE_EXIT_AFTER_STOP_DELAY", "5000");
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

    let child =
        agent_runtime_core::spawn_isolated(cmd, ChildLifetime::KillOnDrop).map_err(|e| {
            CcError::SpawnFailed {
                binary: claude.clone(),
                source: e,
            }
        })?;
    let pid = global_types::Pid::new(child.id().ok_or(CcError::StreamClosed)?);

    Ok((child, pid, stream_path, stream_file))
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
    if config.caller.starts_with("scout-") {
        cmd.env("DISABLE_LANG_GUARD", "1");
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    if !config.cwd.as_os_str().is_empty() {
        cmd.current_dir(&config.cwd);
    }

    let child = agent_runtime_core::spawn_isolated(cmd, ChildLifetime::Managed).map_err(|e| {
        CcError::SpawnFailed {
            binary: claude.clone(),
            source: e,
        }
    })?;
    let pid = global_types::Pid::new(child.id().ok_or(CcError::StreamClosed)?);

    Ok((child, pid, stream_path))
}
