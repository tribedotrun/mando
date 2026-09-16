use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;

pub type SessionFuture<T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'static>>;
pub type UnitFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Debug, Clone)]
pub struct SessionStructuredOutput(serde_json::Value);

impl From<serde_json::Value> for SessionStructuredOutput {
    fn from(value: serde_json::Value) -> Self {
        Self(value)
    }
}

impl SessionStructuredOutput {
    pub fn parse<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.0.clone())
    }
}

pub type SessionAiResult = global_claude::CcResult<SessionStructuredOutput>;

type StartFn = dyn Fn(SessionStartRequest) -> SessionFuture<SessionAiResult> + Send + Sync;
type FollowUpFn = dyn Fn(SessionFollowUpRequest) -> SessionFuture<SessionAiResult> + Send + Sync;
type CloseAsyncFn = dyn Fn(String) -> UnitFuture + Send + Sync;
type ListSessionsFn = dyn Fn(SessionListQuery) -> SessionFuture<SessionListPage> + Send + Sync;
type SessionByIdFn = dyn Fn(String) -> SessionFuture<Option<api_types::SessionEntry>> + Send + Sync;
type SessionCwdFn = dyn Fn(String) -> SessionFuture<Option<String>> + Send + Sync;
type JsonlPathFn = dyn Fn(String) -> SessionFuture<Option<String>> + Send + Sync;
type MessagesFn = dyn Fn(String, Option<usize>, usize) -> SessionFuture<Option<Vec<global_claude::TranscriptMessage>>>
    + Send
    + Sync;
type ToolUsageFn =
    dyn Fn(String) -> SessionFuture<Option<Vec<global_claude::ToolUsageSummary>>> + Send + Sync;
type SessionCostFn =
    dyn Fn(String) -> SessionFuture<Option<global_claude::SessionCost>> + Send + Sync;
type StreamFn = dyn Fn(String, Option<Vec<String>>) -> SessionFuture<Option<String>> + Send + Sync;
type EventsSnapshotFn = dyn Fn(String) -> SessionFuture<Option<crate::runtime::transcript_access::EventsSnapshot>>
    + Send
    + Sync;
type ToolResultImageFn = dyn Fn(String, String, usize) -> SessionFuture<Option<global_claude::ToolResultImage>>
    + Send
    + Sync;

#[derive(Debug, Clone)]
pub struct SessionStartRequest {
    pub key: String,
    pub prompt: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub idle_ttl: Duration,
    pub call_timeout: Duration,
    pub task_id: Option<i64>,
    pub max_turns: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SessionFollowUpRequest {
    pub key: String,
    pub message: String,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RecoverStats {
    pub recovered: usize,
    pub corrupt: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SessionListRequest {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub category: Option<String>,
    pub caller: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListQuery {
    pub page: usize,
    pub per_page: usize,
    pub category: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionListPage {
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
    pub categories: BTreeMap<String, u64>,
    pub total_cost_usd: f64,
    pub sessions: Vec<api_types::SessionEntry>,
}

pub struct SessionsRuntimeOps {
    pub recover: Arc<dyn Fn() -> RecoverStats + Send + Sync>,
    pub cleanup_expired: Arc<dyn Fn() -> usize + Send + Sync>,
    pub has_session: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub close: Arc<dyn Fn(&str) + Send + Sync>,
    pub close_async: Arc<CloseAsyncFn>,
    pub start_with_item: Arc<StartFn>,
    pub start_replacing: Arc<StartFn>,
    pub follow_up: Arc<FollowUpFn>,
    pub list_sessions: Arc<ListSessionsFn>,
    pub session_by_id: Arc<SessionByIdFn>,
    pub session_cwd: Arc<SessionCwdFn>,
    pub session_jsonl_path: Arc<JsonlPathFn>,
    pub session_messages: Arc<MessagesFn>,
    pub session_tool_usage: Arc<ToolUsageFn>,
    pub session_cost: Arc<SessionCostFn>,
    pub session_stream: Arc<StreamFn>,
    pub events_snapshot: Arc<EventsSnapshotFn>,
    pub tool_result_image: Arc<ToolResultImageFn>,
}

#[derive(Clone)]
pub struct SessionsRuntime {
    recover: Arc<dyn Fn() -> RecoverStats + Send + Sync>,
    cleanup_expired: Arc<dyn Fn() -> usize + Send + Sync>,
    has_session: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    close: Arc<dyn Fn(&str) + Send + Sync>,
    close_async: Arc<CloseAsyncFn>,
    start_with_item: Arc<StartFn>,
    start_replacing: Arc<StartFn>,
    follow_up: Arc<FollowUpFn>,
    list_sessions: Arc<ListSessionsFn>,
    session_by_id: Arc<SessionByIdFn>,
    session_cwd: Arc<SessionCwdFn>,
    session_jsonl_path: Arc<JsonlPathFn>,
    session_messages: Arc<MessagesFn>,
    session_tool_usage: Arc<ToolUsageFn>,
    session_cost: Arc<SessionCostFn>,
    session_stream: Arc<StreamFn>,
    events_snapshot: Arc<EventsSnapshotFn>,
    tool_result_image: Arc<ToolResultImageFn>,
}

impl SessionsRuntime {
    pub fn new(ops: SessionsRuntimeOps) -> Self {
        Self {
            recover: ops.recover,
            cleanup_expired: ops.cleanup_expired,
            has_session: ops.has_session,
            close: ops.close,
            close_async: ops.close_async,
            start_with_item: ops.start_with_item,
            start_replacing: ops.start_replacing,
            follow_up: ops.follow_up,
            list_sessions: ops.list_sessions,
            session_by_id: ops.session_by_id,
            session_cwd: ops.session_cwd,
            session_jsonl_path: ops.session_jsonl_path,
            session_messages: ops.session_messages,
            session_tool_usage: ops.session_tool_usage,
            session_cost: ops.session_cost,
            session_stream: ops.session_stream,
            events_snapshot: ops.events_snapshot,
            tool_result_image: ops.tool_result_image,
        }
    }

    pub fn recover(&self) -> RecoverStats {
        (self.recover)()
    }

    pub fn cleanup_expired(&self) -> usize {
        (self.cleanup_expired)()
    }

    pub fn has_session(&self, key: &str) -> bool {
        (self.has_session)(key)
    }

    pub fn close(&self, key: &str) {
        (self.close)(key)
    }

    #[tracing::instrument(skip_all)]
    pub async fn close_async(&self, key: &str) {
        (self.close_async)(key.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn start_with_item(
        &self,
        request: SessionStartRequest,
    ) -> anyhow::Result<SessionAiResult> {
        (self.start_with_item)(request).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn start_replacing(
        &self,
        request: SessionStartRequest,
    ) -> anyhow::Result<SessionAiResult> {
        (self.start_replacing)(request).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn follow_up(
        &self,
        request: SessionFollowUpRequest,
    ) -> anyhow::Result<SessionAiResult> {
        (self.follow_up)(request).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_sessions(
        &self,
        request: SessionListRequest,
    ) -> anyhow::Result<SessionListPage> {
        let query = SessionListQuery {
            page: request.page.unwrap_or(1).max(1) as usize,
            per_page: request.per_page.unwrap_or(50).max(1) as usize,
            category: request.caller.or(request.category),
            status: request.status,
        };
        (self.list_sessions)(query).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_by_id(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<api_types::SessionEntry>> {
        (self.session_by_id)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_cwd(&self, session_id: &str) -> anyhow::Result<Option<String>> {
        (self.session_cwd)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_jsonl_path(&self, session_id: &str) -> anyhow::Result<Option<String>> {
        (self.session_jsonl_path)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_messages(
        &self,
        session_id: &str,
        limit: Option<usize>,
        offset: usize,
    ) -> anyhow::Result<Option<Vec<global_claude::TranscriptMessage>>> {
        (self.session_messages)(session_id.to_string(), limit, offset).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_tool_usage(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<Vec<global_claude::ToolUsageSummary>>> {
        (self.session_tool_usage)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_cost(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<global_claude::SessionCost>> {
        (self.session_cost)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn session_stream(
        &self,
        session_id: &str,
        types: Option<Vec<String>>,
    ) -> anyhow::Result<Option<String>> {
        (self.session_stream)(session_id.to_string(), types).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn events_snapshot(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<crate::runtime::transcript_access::EventsSnapshot>> {
        (self.events_snapshot)(session_id.to_string()).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn tool_result_image(
        &self,
        session_id: &str,
        tool_use_id: &str,
        image_index: usize,
    ) -> anyhow::Result<Option<global_claude::ToolResultImage>> {
        (self.tool_result_image)(session_id.to_string(), tool_use_id.to_string(), image_index).await
    }
}
