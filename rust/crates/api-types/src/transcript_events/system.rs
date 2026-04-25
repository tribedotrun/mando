//! System-level transcript events.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::EventMeta;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemInitEvent {
    pub meta: EventMeta,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<CcPermissionMode>,
    pub tools: Vec<String>,
    pub slash_commands: Vec<String>,
    pub mcp_servers: Vec<McpServerStatus>,
    pub output_style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServerStatus {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CcPermissionMode {
    Default,
    AcceptEdits,
    BypassPermissions,
    Plan,
    DontAsk,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemCompactBoundaryEvent {
    pub meta: EventMeta,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemStatusEvent {
    pub meta: EventMeta,
    pub status: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemApiRetryEvent {
    pub meta: EventMeta,
    pub message: Option<String>,
    #[ts(type = "number | null")]
    pub retry_in_ms: Option<u64>,
    #[ts(type = "number | null")]
    pub attempt: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemLocalCommandOutputEvent {
    pub meta: EventMeta,
    pub command: Option<String>,
    pub output: String,
}

/// CC hook-lifecycle trace (SessionStart/PreCompact/UserPromptSubmit invocations).
/// These fire every session and are mostly plumbing — the viewer collapses them
/// out of the main flow, but they travel the wire so debuggers can still surface
/// hook-attributed failures.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemHookEvent {
    pub meta: EventMeta,
    pub phase: HookPhase,
    pub hook_id: Option<String>,
    pub hook_name: Option<String>,
    pub hook_event: Option<String>,
    pub output: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    Started,
    Response,
}

/// Rate-limit signal CC emits when approaching a quota ceiling. Mirrors the
/// `rate_limit_event` top-level type with the payload preserved as stringified
/// JSON (server-side schema drifts quickly; we surface it verbatim).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemRateLimitEvent {
    pub meta: EventMeta,
    pub info: String,
}
