mod binary;
mod broken_session;
mod config;
mod credentials;
mod error;
mod json_parse;
mod message;
mod oneshot;
mod pricing;
mod process;
mod protocol;
mod session;
mod stream;
mod stream_symptoms;
mod transcript;
mod transcript_events;

pub use agent_runtime_core::{
    apply_codex_binary_env, get_cpu_time, is_process_alive, is_stream_meta_finished_at,
    kill_process, resolve_codex_binary, update_stream_meta_status_at, write_stream_meta_at,
    ResolvedCodexBinary, SessionMeta, DAEMON_ENV_STRIP,
};
pub use api_types::ResultOutcome;
pub use binary::resolve_claude_binary;
pub use broken_session::{stream_broken_session_symptom, BrokenSessionMatch, BrokenSessionOrigin};
pub use config::{CcConfig, CcConfigBuilder, Effort, PermissionMode};
pub use credentials::{credential_id, with_credential};
pub use error::{CcError, ErrorClass};
pub use json_parse::{parse_llm_json, parse_llm_json_as};
pub use message::{
    AssistantMessage, CcMessage, ContentBlock, InitMessage, RateLimitEvent, RateLimitStatus,
    ResultMessage,
};
pub use oneshot::CcOneShot;
pub use pricing::{fallback_rate, rate_for_model, ModelRate};
pub use process::spawn_detached;
pub use session::CcSession;
pub use stream::{
    get_last_assistant_text, get_stream_cost, get_stream_file_size, get_stream_result,
    has_rate_limit_rejection, is_clean_result, last_rate_limit_status, result_outcome,
    stream_has_broken_session, stream_stale_seconds, write_error_result, write_interrupted_result,
    RateLimitRejection, StreamCostInfo, StreamRateLimitInfo,
};
pub use stream_symptoms::{CcStreamSymptom, StreamSymptomMatcher, StreamSymptomRule};
pub use transcript::{
    parse_messages, session_cost, session_cost_or_estimate, tool_usage, ModelUsage, SessionCost,
    ToolUsageSummary, TranscriptMessage,
};
pub use transcript_events::{parse_events_from_offset, parse_events_with_size};

/// Opaque wrapper for the raw Claude session JSON envelope.
/// Kept as a named type so callers cannot accidentally inspect internals
/// via `serde_json::Value` escape hatches.
#[derive(Debug)]
pub struct CcEnvelope(pub serde_json::Value);

#[derive(Debug)]
pub struct CcResult<T> {
    pub text: String,
    pub structured: Option<T>,
    pub session_id: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub duration_api_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub errors: Vec<String>,
    pub envelope: CcEnvelope,
    pub stream_path: std::path::PathBuf,
    pub rate_limit: Option<RateLimitEvent>,
    pub pid: global_types::Pid,
    /// Settings-managed credential id whose OAuth token billed this result.
    /// Propagated from `CcConfig.credential_id` so per-credential cost
    /// accounting stays accurate after failover (the final successful
    /// attempt may have used a different credential than the first).
    /// `None` means ambient auth (no credential rows configured).
    pub credential_id: Option<i64>,
}

pub fn write_stream_meta(meta: &SessionMeta<'_>, status: &str) {
    let meta_path = global_infra::paths::stream_meta_path_for_session(meta.session_id);
    agent_runtime_core::write_stream_meta_at(&meta_path, meta, status);
}

pub fn update_stream_meta_status(session_id: &str, status: &str, cost_usd: Option<f64>) {
    let meta_path = global_infra::paths::stream_meta_path_for_session(session_id);
    agent_runtime_core::update_stream_meta_status_at(&meta_path, session_id, status, cost_usd);
}

pub fn is_session_finished(session_id: &str) -> bool {
    let meta_path = global_infra::paths::stream_meta_path_for_session(session_id);
    agent_runtime_core::is_stream_meta_finished_at(&meta_path)
}
