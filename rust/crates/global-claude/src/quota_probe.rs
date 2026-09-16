//! Read subscription windows from a minimal, isolated Claude print session.
//!
//! Claude Code 2.1.270 emits the Fable bucket as
//! `unifiedWindows.seven_day_overage_included`. A Haiku inference request
//! does not expose that bucket for setup-token credentials.

use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

use crate::{CcMessage, RateLimitStatus};

/// One subscription window emitted by Claude Code. Utilization may exceed 1.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuotaWindow {
    pub utilization: f64,
    pub resets_at: i64,
}

/// Validated subscription usage, without conversation or credential data.
pub struct QuotaSnapshot {
    pub five_hour: QuotaWindow,
    pub seven_day: QuotaWindow,
    pub seven_day_fable: Option<QuotaWindow>,
    pub status: RateLimitStatus,
    pub representative_claim: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum QuotaProbeError {
    #[error("Claude usage probe I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Claude usage probe timed out after 90 seconds")]
    Timeout,
    #[error("Claude credential authentication failed")]
    Unauthorized,
    #[error("Claude credential is rate limited")]
    RateLimited {
        resets_at: Option<u64>,
        claim: Option<String>,
    },
    #[error("Claude usage probe response invalid: {0}")]
    Parse(String),
}

/// Run one no-tools Fable turn using only the supplied OAuth credential.
///
/// The temporary config and working directory exclude user/repository
/// instructions, hooks, plugins, MCP servers, and persisted sessions. The
/// child is killed if this future is cancelled or reaches its timeout.
#[tracing::instrument(skip_all)]
pub async fn probe_quota(access_token: &str) -> Result<QuotaSnapshot, QuotaProbeError> {
    let directory = tempfile::Builder::new().prefix("mando-usage-").tempdir()?;
    let mut command = tokio::process::Command::new(crate::resolve_claude_binary());
    command.env_clear();
    for name in ["HOME", "PATH", "TMPDIR", "SYSTEMROOT"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .current_dir(directory.path())
        .env("CLAUDE_CONFIG_DIR", directory.path())
        .env("CLAUDE_CODE_OAUTH_TOKEN", access_token)
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .args([
            "-p",
            "Reply with OK.",
            "--model",
            "claude-fable-5-1",
            "--effort",
            "low",
            "--max-turns",
            "1",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--setting-sources",
            "",
            "--settings",
            "{\"disableAllHooks\":true}",
            "--disable-slash-commands",
            "--system-prompt",
            "Reply briefly.",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child =
        agent_runtime_core::spawn_isolated(command, agent_runtime_core::ChildLifetime::KillOnDrop)?;
    let output = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output())
        .await
        .map_err(|_| QuotaProbeError::Timeout)??;
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|_| QuotaProbeError::Parse("stdout was not UTF-8".into()))?;
    let mut latest = None;
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let value = serde_json::from_str(line)
            .map_err(|error| QuotaProbeError::Parse(error.to_string()))?;
        match CcMessage::parse(value) {
            CcMessage::RateLimit(event) => {
                latest = Some(snapshot_from_event(event));
            }
            CcMessage::Assistant(message)
                if matches!(
                    message.error.as_deref(),
                    Some("authentication_failed" | "authentication_error")
                ) =>
            {
                return Err(QuotaProbeError::Unauthorized);
            }
            _ => {}
        }
    }
    // Rejected requests can exit unsuccessfully while carrying valid windows.
    latest.unwrap_or_else(|| {
        Err(QuotaProbeError::Parse(format!(
            "no subscription windows returned (exit {}); check Claude Code version and credential",
            output.status
        )))
    })
}

fn snapshot_from_event(event: crate::RateLimitEvent) -> Result<QuotaSnapshot, QuotaProbeError> {
    if !event.status.is_known() {
        return Err(QuotaProbeError::Parse("unknown quota status".into()));
    }
    let parse = || {
        let windows = event
            .raw
            .get("rate_limit_info")
            .and_then(|info| info.get("unifiedWindows"))
            .ok_or_else(|| {
                QuotaProbeError::Parse("missing unifiedWindows; update Claude Code".into())
            })?;
        // Project the external bucket catalog, which can grow independently
        // of the three windows Mando supports. Unknown buckets remain ignored.
        let five_hour = parse_window(windows, "five_hour")?
            .ok_or_else(|| QuotaProbeError::Parse("missing five_hour".into()))?;
        let seven_day = parse_window(windows, "seven_day")?
            .ok_or_else(|| QuotaProbeError::Parse("missing seven_day".into()))?;
        let seven_day_fable = parse_window(windows, "seven_day_overage_included")?;
        Ok(QuotaSnapshot {
            five_hour,
            seven_day,
            seven_day_fable,
            status: event.status.clone(),
            representative_claim: event.rate_limit_type.clone(),
        })
    };
    match parse() {
        Err(_) if event.status == RateLimitStatus::Rejected => Err(QuotaProbeError::RateLimited {
            resets_at: event.resets_at,
            claim: event.rate_limit_type,
        }),
        result => result,
    }
}

fn parse_window(
    windows: &serde_json::Value,
    name: &str,
) -> Result<Option<QuotaWindow>, QuotaProbeError> {
    let Some(value) = windows.get(name).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let window = QuotaWindow {
        utilization: value
            .get("utilization")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| QuotaProbeError::Parse(format!("invalid {name} utilization")))?,
        resets_at: value
            .get("resetsAt")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| QuotaProbeError::Parse(format!("invalid {name} reset")))?,
    };
    if !window.utilization.is_finite() || window.utilization < 0.0 || window.resets_at <= 0 {
        return Err(QuotaProbeError::Parse(format!("invalid {name}")));
    }
    Ok(Some(window))
}
