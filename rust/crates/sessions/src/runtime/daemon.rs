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
    pub session_cwd: Arc<SessionCwdFn>,
    pub session_jsonl_path: Arc<JsonlPathFn>,
    pub session_messages: Arc<MessagesFn>,
    pub session_tool_usage: Arc<ToolUsageFn>,
    pub session_cost: Arc<SessionCostFn>,
    pub session_stream: Arc<StreamFn>,
    pub events_snapshot: Arc<EventsSnapshotFn>,
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
    session_cwd: Arc<SessionCwdFn>,
    session_jsonl_path: Arc<JsonlPathFn>,
    session_messages: Arc<MessagesFn>,
    session_tool_usage: Arc<ToolUsageFn>,
    session_cost: Arc<SessionCostFn>,
    session_stream: Arc<StreamFn>,
    events_snapshot: Arc<EventsSnapshotFn>,
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
            session_cwd: ops.session_cwd,
            session_jsonl_path: ops.session_jsonl_path,
            session_messages: ops.session_messages,
            session_tool_usage: ops.session_tool_usage,
            session_cost: ops.session_cost,
            session_stream: ops.session_stream,
            events_snapshot: ops.events_snapshot,
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
    pub async fn start_ops(&self, request: SessionStartRequest) -> anyhow::Result<SessionAiResult> {
        self.start_replacing(request).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn send_ops_message(
        &self,
        request: SessionFollowUpRequest,
    ) -> anyhow::Result<Option<SessionAiResult>> {
        if !self.has_session(&request.key) {
            return Ok(None);
        }
        self.follow_up(request).await.map(Some)
    }

    #[tracing::instrument(skip_all)]
    pub async fn end_ops(&self, key: &str) -> bool {
        if !self.has_session(key) {
            return false;
        }
        self.close_async(key).await;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    fn test_runtime(
        has_session: bool,
        follow_up_called: Arc<AtomicBool>,
        close_called: Arc<AtomicBool>,
        captured_query: Arc<Mutex<Option<SessionListQuery>>>,
    ) -> SessionsRuntime {
        SessionsRuntime::new(SessionsRuntimeOps {
            recover: Arc::new(RecoverStats::default),
            cleanup_expired: Arc::new(|| 0),
            has_session: Arc::new(move |_| has_session),
            close: Arc::new(|_| {}),
            close_async: Arc::new(move |_| {
                let close_called = close_called.clone();
                Box::pin(async move {
                    close_called.store(true, Ordering::Relaxed);
                })
            }),
            start_with_item: Arc::new(|_| {
                Box::pin(async { Err(anyhow::anyhow!("unused in test")) })
            }),
            start_replacing: Arc::new(|_| {
                Box::pin(async { Err(anyhow::anyhow!("unused in test")) })
            }),
            follow_up: Arc::new(move |_| {
                let follow_up_called = follow_up_called.clone();
                Box::pin(async move {
                    follow_up_called.store(true, Ordering::Relaxed);
                    Ok(global_claude::CcResult {
                        text: "ok".into(),
                        structured: None,
                        session_id: "sess-1".into(),
                        cost_usd: Some(1.25),
                        duration_ms: Some(42),
                        duration_api_ms: None,
                        num_turns: None,
                        errors: Vec::new(),
                        envelope: global_claude::CcEnvelope(serde_json::json!({})),
                        stream_path: PathBuf::from("/tmp/stream.jsonl"),
                        rate_limit: None,
                        pid: 0u32.into(),
                        credential_id: None,
                    })
                })
            }),
            list_sessions: Arc::new(move |query| {
                let captured_query = captured_query.clone();
                Box::pin(async move {
                    *captured_query.lock().unwrap() = Some(query);
                    Ok(SessionListPage {
                        total: 0,
                        page: 1,
                        per_page: 50,
                        total_pages: 1,
                        categories: BTreeMap::new(),
                        total_cost_usd: 0.0,
                        sessions: Vec::new(),
                    })
                })
            }),
            session_cwd: Arc::new(|_| Box::pin(async { Ok(None) })),
            session_jsonl_path: Arc::new(|_| Box::pin(async { Ok(None) })),
            session_messages: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
            session_tool_usage: Arc::new(|_| Box::pin(async { Ok(None) })),
            session_cost: Arc::new(|_| Box::pin(async { Ok(None) })),
            session_stream: Arc::new(|_, _| Box::pin(async { Ok(None) })),
            events_snapshot: Arc::new(|_| Box::pin(async { Ok(None) })),
        })
    }

    #[tokio::test]
    async fn list_sessions_normalizes_query_defaults() {
        let follow_up_called = Arc::new(AtomicBool::new(false));
        let close_called = Arc::new(AtomicBool::new(false));
        let captured_query = Arc::new(Mutex::new(None));
        let runtime = test_runtime(
            false,
            follow_up_called,
            close_called,
            captured_query.clone(),
        );

        runtime
            .list_sessions(SessionListRequest {
                page: Some(0),
                per_page: Some(0),
                category: Some("worker".into()),
                caller: Some("ops".into()),
                status: Some("running".into()),
            })
            .await
            .unwrap();

        assert_eq!(
            *captured_query.lock().unwrap(),
            Some(SessionListQuery {
                page: 1,
                per_page: 1,
                category: Some("ops".into()),
                status: Some("running".into()),
            })
        );
    }

    #[tokio::test]
    async fn send_ops_message_returns_none_when_session_missing() {
        let follow_up_called = Arc::new(AtomicBool::new(false));
        let close_called = Arc::new(AtomicBool::new(false));
        let captured_query = Arc::new(Mutex::new(None));
        let runtime = test_runtime(
            false,
            follow_up_called.clone(),
            close_called,
            captured_query,
        );

        let result = runtime
            .send_ops_message(SessionFollowUpRequest {
                key: "ops".into(),
                message: "hello".into(),
                cwd: PathBuf::from("/tmp"),
            })
            .await
            .unwrap();

        assert!(result.is_none());
        assert!(!follow_up_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn end_ops_returns_false_when_session_missing() {
        let follow_up_called = Arc::new(AtomicBool::new(false));
        let close_called = Arc::new(AtomicBool::new(false));
        let captured_query = Arc::new(Mutex::new(None));
        let runtime = test_runtime(
            false,
            follow_up_called,
            close_called.clone(),
            captured_query,
        );

        let ended = runtime.end_ops("ops").await;

        assert!(!ended);
        assert!(!close_called.load(Ordering::Relaxed));
    }
}
