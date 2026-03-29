//! `CcSession` — multi-turn bidirectional CC session.
//!
//! Stdin stays open. Send follow-up messages via `send_message()`.
//! Hooks work via the control protocol.

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

use crate::config::CcConfig;
use crate::message::{CcMessage, ResultMessage};
use crate::protocol;
use crate::CcResult;

/// A long-lived bidirectional CC session.
pub struct CcSession {
    child: tokio::process::Child,
    /// Persistent buffered reader over stdout — survives across recv_result() calls
    /// so buffered data is not lost in multi-turn sessions.
    stdout_reader: Option<BufReader<tokio::process::ChildStdout>>,
    pid: u32,
    session_id: String,
    stream_path: PathBuf,
    stream_file: std::fs::File,
    config: CcConfig,
    /// Most recent rate limit event from the CLI (if any).
    last_rate_limit: Option<crate::message::RateLimitEvent>,
}

impl CcSession {
    /// Spawn a new CC session with stream-json bidirectional I/O.
    pub async fn spawn(config: CcConfig) -> Result<Self> {
        let session_id = config.effective_session_id();

        let (mut child, pid, stream_path, _stderr_path) =
            crate::process::spawn_process(&config, &session_id).await?;

        // Open stream file for tee-writing stdout lines.
        let stream_file = if config.resume_session_id.is_some() {
            std::fs::File::options()
                .create(true)
                .append(true)
                .open(&stream_path)
                .with_context(|| format!("open stream for tee: {}", stream_path.display()))?
        } else {
            std::fs::File::create(&stream_path)
                .with_context(|| format!("create stream for tee: {}", stream_path.display()))?
        };

        // Write meta sidecar.
        crate::write_stream_meta(
            &crate::SessionMeta {
                session_id: &session_id,
                caller: &config.caller,
                task_id: &config.task_id,
                worker_name: &config.worker_name,
                project: &config.project,
                cwd: &config.cwd.display().to_string(),
            },
            "running",
        );

        info!(
            module = "mando-cc",
            caller = %config.caller,
            session_id = %session_id,
            pid = pid,
            "session spawned"
        );

        // Take stdout immediately and wrap in a persistent BufReader.
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("stdout not piped"))?;
        let stdout_reader = Some(BufReader::new(stdout));

        Ok(Self {
            child,
            stdout_reader,
            pid,
            session_id,
            stream_path,
            stream_file,
            config,
            last_rate_limit: None,
        })
    }

    /// Send a user message to the session via stdin.
    pub async fn send_message(&mut self, content: &str) -> Result<()> {
        let msg = protocol::user_message(content);
        let line = serde_json::to_string(&msg)? + "\n";

        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("stdin closed"))?;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;

        Ok(())
    }

    /// Read messages from stdout until a result message arrives.
    ///
    /// Returns the typed result. All intermediate messages are written to the
    /// stream JSONL file.
    pub async fn recv_result(&mut self) -> Result<CcResult> {
        let start = std::time::Instant::now();

        // Use the persistent BufReader to preserve buffered data across calls.
        let mut reader = self
            .stdout_reader
            .take()
            .ok_or_else(|| anyhow::anyhow!("stdout closed"))?;
        let mut line_buf = String::new();

        let result = loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf).await?;
            if bytes_read == 0 {
                // EOF — process exited. Put reader back and handle.
                self.stdout_reader = Some(reader);
                let elapsed = start.elapsed();
                return self.handle_eof(elapsed).await;
            }

            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Tee to stream file.
            use std::io::Write;
            writeln!(self.stream_file, "{trimmed}").ok();

            // Parse message.
            let val: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue, // Skip non-JSON debug lines.
            };

            let msg = CcMessage::parse(val);
            match msg {
                CcMessage::Init(init) => {
                    if !init.session_id.is_empty() {
                        self.session_id = init.session_id;
                    }
                }
                CcMessage::ControlRequest(cr) => {
                    // Dispatch through hooks for can_use_tool and hook_callback.
                    let response = match cr.subtype.as_str() {
                        "initialize" => protocol::control_response_init(&cr.request_id),
                        "can_use_tool" | "hook_callback" => crate::hooks::dispatch_hook(
                            &self.config.hooks,
                            &cr.subtype,
                            &cr.request_id,
                            &cr.payload,
                        ),
                        _ => protocol::control_response_init(&cr.request_id),
                    };
                    let resp_line = serde_json::to_string(&response)? + "\n";
                    if let Some(stdin) = self.child.stdin.as_mut() {
                        stdin.write_all(resp_line.as_bytes()).await?;
                        stdin.flush().await?;
                    }
                }
                CcMessage::RateLimit(rl) => {
                    let status_str = match &rl.status {
                        crate::message::RateLimitStatus::Allowed => "allowed",
                        crate::message::RateLimitStatus::AllowedWarning => "allowed_warning",
                        crate::message::RateLimitStatus::Rejected => "rejected",
                        crate::message::RateLimitStatus::Unknown(s) => s.as_str(),
                    };
                    match &rl.status {
                        crate::message::RateLimitStatus::Rejected => {
                            warn!(
                                module = "mando-cc",
                                session_id = %self.session_id,
                                caller = %self.config.caller,
                                status = status_str,
                                utilization = rl.utilization.unwrap_or(0.0),
                                "rate limited — request rejected"
                            );
                        }
                        crate::message::RateLimitStatus::AllowedWarning => {
                            info!(
                                module = "mando-cc",
                                session_id = %self.session_id,
                                caller = %self.config.caller,
                                status = status_str,
                                utilization = rl.utilization.unwrap_or(0.0),
                                "rate limit warning — approaching limit"
                            );
                        }
                        _ => {}
                    }
                    self.last_rate_limit = Some(rl);
                }
                CcMessage::Result(result) => {
                    let elapsed = start.elapsed();
                    break self.build_result(result, elapsed);
                }
                _ => {}
            }
        };

        // Put reader back for future recv_result calls.
        self.stdout_reader = Some(reader);

        Ok(result)
    }

    /// Handle EOF (process exited) during recv_result.
    async fn handle_eof(&self, elapsed: std::time::Duration) -> Result<CcResult> {
        // Try to extract result from stream file.
        if let Some(result_val) = crate::stream::get_stream_result(&self.stream_path) {
            let result_msg = match CcMessage::parse(result_val) {
                CcMessage::Result(r) => r,
                _ => anyhow::bail!("stream result is not a result message"),
            };
            return Ok(self.build_result(result_msg, elapsed));
        }

        // Fallback to last assistant text.
        let text = crate::stream::get_last_assistant_text(&self.stream_path).unwrap_or_default();
        if !text.is_empty() {
            warn!(
                module = "mando-cc",
                session_id = %self.session_id,
                "EOF with no result event, recovered from last assistant text"
            );
            return Ok(CcResult {
                text,
                structured: None,
                session_id: self.session_id.clone(),
                cost_usd: None,
                duration_ms: Some(elapsed.as_millis() as u64),
                duration_api_ms: None,
                num_turns: None,
                errors: Vec::new(),
                envelope: serde_json::Value::Null,
                stream_path: self.stream_path.clone(),
                rate_limit: self.last_rate_limit.clone(),
            });
        }

        crate::update_stream_meta_status(&self.session_id, "failed", None);
        anyhow::bail!(
            "CC exited with no result event in stream: {}",
            self.stream_path.display()
        )
    }

    fn build_result(&self, result: ResultMessage, elapsed: std::time::Duration) -> CcResult {
        let cost = result.total_cost_usd;
        let duration = result.duration_ms.or(Some(elapsed.as_millis() as u64));
        let actual_sid = if result.session_id.is_empty() {
            self.session_id.clone()
        } else {
            result.session_id.clone()
        };

        crate::update_stream_meta_status(&actual_sid, "done", cost);

        info!(
            module = "mando-cc",
            caller = %self.config.caller,
            session_id = %actual_sid,
            cost_usd = cost.unwrap_or(0.0),
            duration_ms = duration.unwrap_or(0),
            "session result received"
        );

        CcResult {
            text: result.result_text,
            structured: result.structured_output,
            session_id: actual_sid,
            cost_usd: cost,
            duration_ms: duration,
            duration_api_ms: result.duration_api_ms,
            num_turns: result.num_turns,
            errors: result.errors,
            envelope: result.raw,
            stream_path: self.stream_path.clone(),
            rate_limit: self.last_rate_limit.clone(),
        }
    }

    /// Get the process ID.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the stream file path.
    pub fn stream_path(&self) -> &std::path::Path {
        &self.stream_path
    }

    /// Gracefully close the session — close stdin, wait for exit.
    pub async fn close(mut self) -> Result<()> {
        // Close stdin to signal EOF.
        drop(self.child.stdin.take());

        // Wait up to 5s for graceful exit.
        match tokio::time::timeout(std::time::Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => warn!(module = "mando-cc", %e, "wait error on close"),
            Err(_) => {
                // Timeout — kill.
                crate::process::kill_process(self.pid).await?;
            }
        }
        Ok(())
    }
}
