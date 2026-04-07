//! Captain-specific worker spawning wrappers.
//!
//! Every other helper in this module was a one-line `mando_cc::X(args)`
//! passthrough adding no abstraction, so those have been removed and call
//! sites now import `mando_cc::*` directly. The two genuine wrappers that
//! remain are `spawn_worker_process` / `resume_worker_process`, which also
//! wire captain's `watch_worker_exit` side effect and therefore cannot be
//! replaced by a direct call.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Shared helper for spawn and resume. Builds a CcConfig, applies env
/// overrides, spawns detached, wires up worker-exit watching, and returns
/// `(pid, stream_path)`.
async fn spawn_worker_impl(
    prompt: &str,
    cwd: &Path,
    model: &str,
    session_or_resume_id: &str,
    env_overrides: &HashMap<String, String>,
    fallback_model: Option<&str>,
    resume: bool,
) -> Result<(mando_types::Pid, PathBuf)> {
    let mut builder = mando_cc::CcConfig::builder()
        .model(model)
        .effort(mando_cc::Effort::Max)
        .cwd(cwd);
    if resume {
        builder = builder.resume(session_or_resume_id);
    } else {
        builder = builder.session_id(session_or_resume_id);
    }
    if let Some(fb) = fallback_model {
        builder = builder.fallback_model(fb);
    }
    for (k, v) in env_overrides {
        builder = builder.env(k, v);
    }
    let (child, pid, stream_path) =
        mando_cc::spawn_detached(&builder.build(), prompt, session_or_resume_id).await?;
    crate::watch_worker_exit(child, pid, session_or_resume_id);
    Ok((pid, stream_path))
}

/// Spawn a long-lived worker CC process with stream-json output.
///
/// Thin wrapper around `mando_cc::spawn_detached` that also wires captain's
/// `watch_worker_exit` side effect. Output is written to
/// `cc-streams/{session_id}.jsonl`. Returns `(pid, stdout_path)`.
pub(crate) async fn spawn_worker_process(
    prompt: &str,
    cwd: &Path,
    model: &str,
    session_id: &str,
    env_overrides: &HashMap<String, String>,
    fallback_model: Option<&str>,
) -> Result<(mando_types::Pid, PathBuf)> {
    spawn_worker_impl(
        prompt,
        cwd,
        model,
        session_id,
        env_overrides,
        fallback_model,
        false,
    )
    .await
}

/// Resume a worker with --resume instead of --session-id.
pub async fn resume_worker_process(
    message: &str,
    cwd: &Path,
    model: &str,
    resume_session_id: &str,
    env_overrides: &HashMap<String, String>,
    fallback_model: Option<&str>,
) -> Result<(mando_types::Pid, PathBuf)> {
    spawn_worker_impl(
        message,
        cwd,
        model,
        resume_session_id,
        env_overrides,
        fallback_model,
        true,
    )
    .await
}

/// Kill a worker process; delegates to `mando_cc::kill_process`.
///
/// Kept as a wrapper only because gateway routes call it via
/// `mando_captain::io::process_manager::kill_worker_process` for API
/// visibility. Direct `mando_cc::kill_process` is used inside the
/// captain crate itself.
pub async fn kill_worker_process(pid: mando_types::Pid) -> Result<()> {
    mando_cc::kill_process(pid).await
}
