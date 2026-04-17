//! `CcOneShot` — single-turn CC invocation.
//!
//! Sends prompt via stdin, waits for result, closes stdin.
//! Hooks still work (stdin open until result arrives).

use tracing::{info, warn};

use crate::config::CcConfig;
use crate::error::CcError;
use crate::CcResult;

/// Single-turn CC invocation.
pub struct CcOneShot;

impl CcOneShot {
    /// Run a one-shot CC invocation with structured output.
    ///
    /// Sends prompt via stdin (not `-p`), waits for result, returns typed output.
    /// Hooks work because stdin stays open until the result message arrives.
    pub async fn run(prompt: &str, config: CcConfig) -> Result<CcResult, CcError> {
        Self::run_with_pid_hook(prompt, config, |_| {}).await
    }

    /// Run with a callback fired immediately after the CC process spawns.
    ///
    /// Use this when you need to register the PID before waiting for the result
    /// (e.g., for liveness tracking in a PID registry).
    pub async fn run_with_pid_hook(
        prompt: &str,
        config: CcConfig,
        on_spawn: impl FnOnce(global_types::Pid),
    ) -> Result<CcResult, CcError> {
        let timeout = config.timeout;
        let caller = config.caller.clone();

        let mut session = crate::CcSession::spawn(config).await?;
        let pid = session.pid();
        let sid = session.session_id().to_string();
        on_spawn(pid);

        // Send the prompt. Internal helpers still use anyhow::Result;
        // normalize into CcError::Other at the public boundary.
        session.send_message(prompt).await.map_err(CcError::Other)?;

        info!(
            module = "mando-cc",
            caller = %caller,
            session_id = %sid,
            pid = %pid,
            timeout_s = timeout.as_secs(),
            "oneshot prompt sent, waiting for result"
        );

        // Wait for result with timeout.
        let result = match tokio::time::timeout(timeout, session.recv_result()).await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                let stream_size = std::fs::metadata(session.stream_path())
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                let pid_alive = crate::process::is_process_alive(pid);
                warn!(
                    module = "mando-cc",
                    caller = %caller,
                    session_id = %sid,
                    pid = %pid,
                    pid_alive,
                    stream_file_bytes = stream_size,
                    error = %e,
                    "oneshot recv_result failed"
                );
                crate::update_stream_meta_status(session.session_id(), "failed", None);
                // Close cleanly before propagating.
                let _ = session.close().await;
                return Err(CcError::Other(e));
            }
            Err(_) => {
                // Timeout — kill and bail.
                let session_id = session.session_id().to_string();
                let stream_path = session.stream_path().to_path_buf();
                let stream_size = std::fs::metadata(&stream_path)
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                let pid_alive = crate::process::is_process_alive(pid);
                crate::update_stream_meta_status(&session_id, "timeout", None);

                warn!(
                    module = "mando-cc",
                    caller = %caller,
                    session_id = %session_id,
                    pid = %pid,
                    pid_alive,
                    timeout_s = timeout.as_secs(),
                    stream_file_bytes = stream_size,
                    "oneshot timed out"
                );

                crate::process::kill_process(pid)
                    .await
                    .map_err(CcError::Other)?;

                return Err(CcError::Other(anyhow::anyhow!(
                    "oneshot timed out after {}s (session={}, stream={})",
                    timeout.as_secs(),
                    session_id,
                    stream_path.display()
                )));
            }
        };

        info!(
            module = "mando-cc",
            caller = %caller,
            session_id = %result.session_id,
            cost_usd = result.cost_usd.unwrap_or(0.0),
            "oneshot complete"
        );

        // Close stdin and wait for process exit.
        let _ = session.close().await;

        Ok(result)
    }
}
