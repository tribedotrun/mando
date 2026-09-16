//! `CcOneShot` — single-turn CC invocation.
//!
//! Sends prompt via stdin, waits for result, closes stdin.
//! Hooks still work (stdin open until result arrives).

use tracing::{debug, info, warn};

use crate::config::CcConfig;
use crate::error::{CcError, ErrorClass};
use crate::CcResult;

/// Single-turn CC invocation.
pub struct CcOneShot;

impl CcOneShot {
    /// Run a one-shot CC invocation with structured output.
    ///
    /// Sends prompt via stdin (not `-p`), waits for result, returns typed output.
    /// Hooks work because stdin stays open until the result message arrives.
    pub async fn run(
        prompt: &str,
        config: CcConfig,
    ) -> Result<CcResult<serde_json::Value>, CcError> {
        Self::run_with_pid_hook(prompt, config, |_, _| {}).await
    }

    /// Run a one-shot CC invocation, retrying transient API failures up to
    /// `max_retries` times with exponential backoff. Fatal failures and
    /// non-API errors surface on the first attempt.
    ///
    /// Each retry re-uses the same `CcConfig` (including any resume session),
    /// so the caller must have already chosen how they want resumed state to
    /// behave before opting in.
    pub async fn run_with_retry(
        prompt: &str,
        config: CcConfig,
        max_retries: u32,
    ) -> Result<CcResult<serde_json::Value>, CcError> {
        Self::run_with_retry_pid_hook(prompt, config, max_retries, |_, _| {}).await
    }

    /// Like `run_with_retry`, but forwards each attempt's spawned PID and
    /// the session id CC actually adopted to `on_spawn`. The hook fires once
    /// per attempt, so a caller that polls a stream file by session id can
    /// re-point at the attempt that is really running — a retry drops the
    /// caller's pre-allocated id and CC mints its own, which would otherwise
    /// leave the poller watching a stream nothing writes to.
    pub async fn run_with_retry_pid_hook<F>(
        prompt: &str,
        config: CcConfig,
        max_retries: u32,
        on_spawn: F,
    ) -> Result<CcResult<serde_json::Value>, CcError>
    where
        F: Fn(global_types::Pid, &str),
    {
        let caller = config.caller.clone();
        retry_loop(&caller, max_retries, |attempt| {
            let per_attempt_hook = |pid, sid: &str| on_spawn(pid, sid);
            // The first attempt keeps the caller's pre-allocated
            // `session_id` so callers that pre-register the id (captain
            // review, captain merge) and then poll that exact stream /
            // pid still see matching output. Only retries clear it, so
            // CC mints a fresh UUID and cannot bail with "Session ID is
            // already in use" on the consumed id from the first attempt.
            let mut per_attempt = config.clone();
            if attempt > 0 {
                per_attempt.session_id = None;
            }
            Self::run_with_pid_hook(prompt, per_attempt, per_attempt_hook)
        })
        .await
    }
}

/// Back-off retry loop: call `mk_attempt` up to `max_retries + 1` times,
/// returning the first `Ok` or the last `Err`. Retries only on
/// `ErrorClass::Transient`; fatal errors short-circuit.
///
/// Public only so unit tests can exercise the classifier-and-backoff
/// behavior without needing a live CC subprocess. `pub(crate)` keeps it
/// out of the crate's public API (verified by `check_public_api_snapshot`).
pub(crate) async fn retry_loop<F, Fut>(
    caller: &str,
    max_retries: u32,
    mut mk_attempt: F,
) -> Result<CcResult<serde_json::Value>, CcError>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<CcResult<serde_json::Value>, CcError>>,
{
    let mut attempt: u32 = 0;
    loop {
        match mk_attempt(attempt).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                if err.classify() != ErrorClass::Transient || attempt >= max_retries {
                    return Err(err);
                }
                // 500ms, 1s, 2s, 4s... capped at 30s.
                let delay_ms = (500u64 << attempt).min(30_000);
                warn!(
                    module = "mando-cc",
                    caller = %caller,
                    attempt = attempt + 1,
                    max_retries,
                    delay_ms,
                    error = %err,
                    "oneshot hit transient error — retrying"
                );
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

// `impl CcOneShot { run_with_pid_hook }` follows this test module. Moving
// the impl in front would shuffle a large block for no behavioral gain;
// allow the lint instead.

impl CcOneShot {
    /// Run with a callback fired immediately after the CC process spawns,
    /// carrying the PID and the session id CC adopted.
    ///
    /// Use this when you need to register the PID before waiting for the result
    /// (e.g., for liveness tracking in a PID registry), or when you poll the
    /// session's stream file and therefore need its real id.
    pub async fn run_with_pid_hook<F>(
        prompt: &str,
        config: CcConfig,
        on_spawn: F,
    ) -> Result<CcResult<serde_json::Value>, CcError>
    where
        F: FnOnce(global_types::Pid, &str),
    {
        let timeout = config.timeout;
        let caller = config.caller.clone();

        let mut session = crate::CcSession::spawn(config).await?;
        let pid = session.pid();
        let sid = session.session_id().to_string();
        on_spawn(pid, &sid);

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
                let pid_alive = crate::is_process_alive(pid);
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
                // build_result has already updated meta for API errors and
                // explicit interruptions. Only remaining variants are
                // failures that still need a terminal meta update here.
                if !matches!(e, CcError::ApiError { .. } | CcError::Interrupted { .. }) {
                    crate::update_stream_meta_status(session.session_id(), "failed", None);
                }
                // Close cleanly before propagating. Close errors are
                // best-effort here: the outer error we are about to return
                // carries the real failure signal.
                if let Err(close_err) = session.close().await {
                    debug!(
                        module = "mando-cc",
                        caller = %caller,
                        error = %close_err,
                        "session.close() failed during oneshot error path",
                    );
                }
                return Err(e);
            }
            Err(_) => {
                // Timeout — kill and bail.
                let session_id = session.session_id().to_string();
                let stream_path = session.stream_path().to_path_buf();
                let stream_size = std::fs::metadata(&stream_path)
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                let pid_alive = crate::is_process_alive(pid);
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

                crate::kill_process(pid).await.map_err(CcError::Other)?;

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

        // Close stdin and wait for process exit. Close errors post-success
        // only affect subprocess teardown; log at debug and return the
        // successful result.
        if let Err(close_err) = session.close().await {
            debug!(
                module = "mando-cc",
                caller = %caller,
                error = %close_err,
                "session.close() failed after oneshot success",
            );
        }

        Ok(result)
    }
}
