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
        Self::run_with_pid_hook(prompt, config, |_| {}).await
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
        Self::run_with_retry_pid_hook(prompt, config, max_retries, |_| {}).await
    }

    /// Like `run_with_retry`, but forwards each attempt's spawned PID to
    /// `on_spawn`. The hook fires once per attempt (so it observes the PID
    /// of the *final* attempt, plus any retried attempts along the way).
    /// Callers that track liveness per-attempt (worker spawn, captain
    /// review, captain merge) can use this; simple callers should use
    /// `run_with_retry`.
    pub async fn run_with_retry_pid_hook<F>(
        prompt: &str,
        config: CcConfig,
        max_retries: u32,
        on_spawn: F,
    ) -> Result<CcResult<serde_json::Value>, CcError>
    where
        F: Fn(global_types::Pid),
    {
        let caller = config.caller.clone();
        retry_loop(&caller, max_retries, |attempt| {
            let per_attempt_hook = |pid| on_spawn(pid);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CcEnvelope;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn dummy_result(session_id: &str) -> CcResult<serde_json::Value> {
        CcResult {
            text: String::new(),
            structured: None,
            session_id: session_id.to_string(),
            cost_usd: None,
            duration_ms: None,
            duration_api_ms: None,
            num_turns: None,
            errors: Vec::new(),
            envelope: CcEnvelope(serde_json::Value::Null),
            stream_path: std::path::PathBuf::new(),
            credential_id: None,
            rate_limit: None,
            pid: global_types::Pid(0),
        }
    }

    #[tokio::test]
    async fn retry_loop_threads_attempt_counter_starting_at_zero() {
        // Callers (`run_with_retry_pid_hook`) rely on `attempt == 0` to
        // decide whether to preserve the caller's pre-allocated
        // session_id. This test pins that contract: first invocation of
        // `mk_attempt` sees 0; the retry sees 1.
        let seen = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
        let seen_clone = seen.clone();
        let attempts_counter = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts_counter.clone();
        let _ = retry_loop("test", 1, |attempt| {
            seen_clone.lock().unwrap().push(attempt);
            let attempts = attempts_clone.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(CcError::ApiError {
                        api_error_status: Some(529),
                        message: "overloaded".into(),
                        session_id: "s-a".into(),
                        credential_id: None,
                    })
                } else {
                    Ok(dummy_result("s-a"))
                }
            }
        })
        .await;
        let observed = seen.lock().unwrap().clone();
        assert_eq!(
            observed,
            vec![0, 1],
            "mk_attempt must see attempt=0 on first call, attempt=1 on retry"
        );
    }

    #[tokio::test]
    async fn retry_loop_retries_transient_then_succeeds() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        let result = retry_loop("test", 2, |_attempt| {
            let attempts = attempts_clone.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(CcError::ApiError {
                        api_error_status: Some(529),
                        message: "overloaded".into(),
                        session_id: "s-t".into(),
                        credential_id: None,
                    })
                } else {
                    Ok(dummy_result("s-t"))
                }
            }
        })
        .await;
        assert!(result.is_ok(), "expected Ok after transient retry");
        assert_eq!(attempts.load(Ordering::SeqCst), 2, "should have run twice");
    }

    #[tokio::test]
    async fn retry_loop_surfaces_fatal_immediately() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        let result = retry_loop("test", 5, |_attempt| {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(CcError::ApiError {
                    api_error_status: Some(400),
                    message: "bad request".into(),
                    session_id: "s-f".into(),
                    credential_id: None,
                })
            }
        })
        .await;
        assert!(result.is_err(), "fatal status must not succeed");
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "fatal must not retry even with max_retries=5"
        );
    }

    #[tokio::test]
    async fn retry_loop_gives_up_after_max_retries() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        let result = retry_loop("test", 1, |_attempt| {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(CcError::ApiError {
                    api_error_status: Some(503),
                    message: "unavailable".into(),
                    session_id: "s-m".into(),
                    credential_id: None,
                })
            }
        })
        .await;
        assert!(result.is_err(), "persistent transient must surface");
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "max_retries=1 allows one original + one retry = 2 total"
        );
    }
}

impl CcOneShot {
    /// Run with a callback fired immediately after the CC process spawns.
    ///
    /// Use this when you need to register the PID before waiting for the result
    /// (e.g., for liveness tracking in a PID registry).
    pub async fn run_with_pid_hook<F>(
        prompt: &str,
        config: CcConfig,
        on_spawn: F,
    ) -> Result<CcResult<serde_json::Value>, CcError>
    where
        F: FnOnce(global_types::Pid),
    {
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
                // build_result has already updated the meta status for an
                // ApiError envelope; for every other variant we still mark
                // the session as failed so obs reflects the outcome.
                if !matches!(e, CcError::ApiError { .. }) {
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
