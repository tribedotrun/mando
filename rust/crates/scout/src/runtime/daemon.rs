use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use anyhow::Result;
use futures_util::FutureExt;
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[path = "daemon_research_runtime.rs"]
mod daemon_research_runtime;

#[derive(Clone)]
pub struct ScoutRuntime {
    settings: Arc<settings::SettingsRuntime>,
    bus: Arc<global_bus::EventBus>,
    pool: sqlx::SqlitePool,
    processing_semaphore: Arc<Semaphore>,
    task_tracker: TaskTracker,
    cancellation_token: CancellationToken,
    qa_session_mgr: Arc<crate::runtime::qa::QaSessionManager>,
}

impl ScoutRuntime {
    pub fn new(
        settings: Arc<settings::SettingsRuntime>,
        bus: Arc<global_bus::EventBus>,
        pool: sqlx::SqlitePool,
        processing_semaphore: Arc<Semaphore>,
        task_tracker: TaskTracker,
        cancellation_token: CancellationToken,
        qa_session_mgr: Arc<crate::runtime::qa::QaSessionManager>,
    ) -> Self {
        Self {
            settings,
            bus,
            pool,
            processing_semaphore,
            task_tracker,
            cancellation_token,
            qa_session_mgr,
        }
    }

    pub fn qa_session_mgr(&self) -> &Arc<crate::runtime::qa::QaSessionManager> {
        &self.qa_session_mgr
    }

    #[tracing::instrument(skip_all)]
    pub async fn publish_telegraph(&self, id: i64) -> Result<String> {
        let workflow = self.settings.load_scout_workflow();
        crate::runtime::dashboard::publish_scout_item_to_telegraph(&self.pool, id, &workflow).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_items(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        item_type: Option<&str>,
        page: Option<usize>,
        per_page: Option<usize>,
    ) -> Result<Value> {
        crate::runtime::dashboard::list_scout_items(
            &self.pool, status, search, item_type, page, per_page,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_item_value(&self, id: i64) -> Result<Value> {
        crate::runtime::dashboard::get_scout_item(&self.pool, id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_article_value(&self, id: i64) -> Result<Value> {
        let workflow = self.settings.load_scout_workflow();
        crate::runtime::dashboard::ensure_scout_article(&self.pool, id, &workflow).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_item(&self, url: &str, title: Option<&str>) -> Result<Value> {
        crate::runtime::dashboard::add_scout_item(&self.pool, url, title).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn process_item(&self, id: Option<i64>) -> Result<Value> {
        let config = self.settings.load_config();
        let workflow = self.settings.load_scout_workflow();
        crate::runtime::dashboard::process_scout(&config, &self.pool, id, &workflow).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn act_on_item(&self, id: i64, project: &str, prompt: Option<&str>) -> Result<Value> {
        let config = self.settings.load_config();
        let workflow = self.settings.load_scout_workflow();
        crate::runtime::dashboard::act_on_scout_item(
            &config, &self.pool, id, project, prompt, &workflow,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn apply_item_command(
        &self,
        id: i64,
        command: crate::service::lifecycle::ScoutItemCommand,
    ) -> Result<()> {
        crate::runtime::dashboard::apply_scout_item_command(&self.pool, id, command).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_item(&self, id: i64) -> Result<Value> {
        crate::runtime::dashboard::delete_scout_item(&self.pool, id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_item_sessions(&self, id: i64) -> Result<Vec<sessions::queries::SessionRow>> {
        sessions::queries::list_sessions_for_scout_item(&self.pool, id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn bulk_apply_item_command(
        &self,
        ids: &[i64],
        command: crate::service::lifecycle::ScoutItemCommand,
    ) -> Value {
        crate::runtime::dashboard::bulk_apply_scout_item_command(&self.pool, ids, command).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn bulk_delete_items(&self, ids: &[i64]) -> Value {
        crate::bulk_delete_scout_items(&self.pool, ids).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn ask_about_item(
        &self,
        id: i64,
        question: &str,
        session_key: Option<&str>,
    ) -> Result<crate::runtime::qa::QaResult> {
        let workflow = self.settings.load_scout_workflow();
        let item = crate::runtime::dashboard::get_scout_item(&self.pool, id).await?;
        let article_data =
            crate::runtime::dashboard::ensure_scout_article(&self.pool, id, &workflow).await?;

        let summary = item["summary"]
            .as_str()
            .unwrap_or("(no summary)")
            .to_string();
        let article = article_data["article"]
            .as_str()
            .unwrap_or("(no article content)")
            .to_string();

        let raw_path = crate::content_path(id);
        let raw_note = if raw_path.exists() {
            Some(format!(
                "The original source content is saved at `{}`. Read it for full detail.",
                raw_path.display()
            ))
        } else {
            None
        };

        self.qa_session_mgr
            .ask(
                question,
                &summary,
                &article,
                raw_note.as_deref(),
                &workflow,
                session_key,
                &self.pool,
            )
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn record_qa_session(
        &self,
        id: i64,
        session_id: &str,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
        credential_id: Option<i64>,
    ) {
        let scout_db = crate::ScoutDb::new(self.pool.clone());
        if let Err(err) = scout_db
            .record_session(
                Some(id),
                session_id,
                "scout-qa",
                cost_usd,
                duration_ms,
                credential_id,
            )
            .await
        {
            tracing::warn!(module = "scout-runtime-daemon", error = %err, "post_scout_ask: failed to record session");
            return;
        }
        self.bus.send(global_bus::BusPayload::Sessions(None));
    }

    #[tracing::instrument(skip_all)]
    pub async fn emit_processed_notification(&self, id: i64) {
        daemon_research_runtime::emit_scout_processed(&self.bus, &self.pool, id).await;
    }

    #[tracing::instrument(skip_all)]
    pub async fn resume_pending_items(&self) {
        match crate::io::queries::scout::list_processable(&self.pool).await {
            Ok(items) if items.is_empty() => {}
            Ok(items) => {
                let count = items.len();
                for item in items {
                    self.spawn_processing(item.id, item.url);
                }
                tracing::info!(
                    module = "startup",
                    count,
                    "resumed pending scout items for processing"
                );
            }
            Err(err) => {
                tracing::warn!(module = "startup", error = %err, "failed to query pending scout items")
            }
        }
    }

    pub fn spawn_processing(&self, id: i64, url: String) {
        let config = self.settings.load_config();
        let workflow = self.settings.load_scout_workflow();
        let pool = self.pool.clone();
        let bus = self.bus.clone();
        let semaphore = self.processing_semaphore.clone();
        self.task_tracker.spawn(async move {
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    // Semaphore closure only happens during daemon shutdown;
                    // the spawn was already in-flight, so log and exit.
                    tracing::warn!(
                        module = "scout-runtime-daemon",
                        error = %e,
                        "processing semaphore closed; abandoning auto-process",
                    );
                    return;
                }
            };
            let result = AssertUnwindSafe(async {
                if let Err(err) = crate::process_scout(&config, &pool, Some(id), &workflow).await {
                    tracing::warn!(module = "scout-runtime-daemon", scout_id = id, error = %err, "auto-process failed");
                    if let Err(db_err) = crate::io::queries::scout::increment_error_count(&pool, id).await {
                        tracing::error!(module = "scout-runtime-daemon", scout_id = id, error = %db_err, "failed to increment error count after process failure");
                    }
                    daemon_research_runtime::emit_scout_process_failed(
                        &bus,
                        id,
                        &url,
                        &err.to_string(),
                    );
                    return;
                }
                let scout_payload = match crate::get_scout_item(&pool, id).await {
                    Ok(value) => Some(value),
                    Err(err) => {
                        tracing::warn!(module = "scout-runtime-daemon", scout_id = id, error = %err, "failed to fetch scout item for SSE event");
                        None
                    }
                };
                let scout_item: Option<api_types::ScoutItem> =
                    scout_payload.and_then(|v| serde_json::from_value(v).ok());
                bus.send(global_bus::BusPayload::Scout(Some(api_types::ScoutEventData {
                    action: Some("updated".into()),
                    item: scout_item,
                    id: Some(id),
                })));
                daemon_research_runtime::emit_scout_processed(&bus, &pool, id).await;
            })
            .catch_unwind()
            .await;
            if let Err(panic) = result {
                tracing::error!(module = "scout-runtime-daemon", scout_id = id, ?panic, "auto-process panicked");
            }
        });
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }
}
