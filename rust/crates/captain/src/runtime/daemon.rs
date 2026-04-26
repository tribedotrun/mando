use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{Notify, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::io::task_store::TaskStore;

#[path = "daemon_auto_title.rs"]
mod daemon_auto_title;
#[path = "daemon_background.rs"]
mod daemon_background;
#[path = "daemon_control_runtime.rs"]
mod daemon_control_runtime;
#[path = "daemon_task_lifecycle.rs"]
mod daemon_task_lifecycle;
#[path = "daemon_task_runtime.rs"]
mod daemon_task_runtime;
#[path = "daemon_task_runtime_clarifier.rs"]
mod daemon_task_runtime_clarifier;
#[path = "daemon_transport_runtime.rs"]
mod daemon_transport_runtime;
#[path = "daemon_workbench_api_runtime.rs"]
mod daemon_workbench_api_runtime;
#[path = "workbench_runtime.rs"]
pub(crate) mod workbench_runtime;

const DEGRADED_FAILURE_THRESHOLD: u32 = 5;

pub struct CaptainRuntimeDeps {
    pub settings: Arc<settings::SettingsRuntime>,
    pub bus: Arc<global_bus::EventBus>,
    pub task_store: Arc<RwLock<TaskStore>>,
    pub pool: SqlitePool,
    pub task_tracker: TaskTracker,
    pub cancellation_token: CancellationToken,
    pub auto_title_notify: Arc<Notify>,
    pub cleanup_expired_sessions: Arc<dyn Fn() -> usize + Send + Sync>,
}

#[derive(Clone)]
pub struct CaptainRuntime {
    settings: Arc<settings::SettingsRuntime>,
    bus: Arc<global_bus::EventBus>,
    task_store: Arc<RwLock<TaskStore>>,
    pool: SqlitePool,
    task_tracker: TaskTracker,
    cancellation_token: CancellationToken,
    auto_title_notify: Arc<Notify>,
    cleanup_expired_sessions: Arc<dyn Fn() -> usize + Send + Sync>,
    degraded: Arc<AtomicBool>,
}

impl CaptainRuntime {
    pub fn new(deps: CaptainRuntimeDeps) -> Self {
        Self {
            settings: deps.settings,
            bus: deps.bus,
            task_store: deps.task_store,
            pool: deps.pool,
            task_tracker: deps.task_tracker,
            cancellation_token: deps.cancellation_token,
            auto_title_notify: deps.auto_title_notify,
            cleanup_expired_sessions: deps.cleanup_expired_sessions,
            degraded: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_background_loops(&self) {
        daemon_background::spawn_auto_tick(self);
        daemon_background::spawn_workbench_cleanup(self);
        daemon_background::spawn_credential_usage_poll(self);
        daemon_auto_title::spawn(self);
    }

    pub fn settings(&self) -> &Arc<settings::SettingsRuntime> {
        &self.settings
    }

    #[tracing::instrument(skip_all)]
    pub async fn drain_pending_lifecycle_effects(&self) -> anyhow::Result<()> {
        super::lifecycle_effects::drain_pending(
            &self.pool,
            Some(self.bus.as_ref()),
            &self.task_store,
        )
        .await
    }

    pub fn bus(&self) -> &Arc<global_bus::EventBus> {
        &self.bus
    }

    pub fn task_store(&self) -> &Arc<RwLock<TaskStore>> {
        &self.task_store
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn task_tracker(&self) -> &TaskTracker {
        &self.task_tracker
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    pub fn auto_title_notify(&self) -> &Arc<Notify> {
        &self.auto_title_notify
    }

    pub(crate) fn cleanup_expired_sessions(&self) -> usize {
        (self.cleanup_expired_sessions)()
    }

    pub(crate) fn set_health_degraded(&self, degraded: bool) {
        self.degraded.store(degraded, Ordering::Relaxed);
    }

    pub fn health_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    #[tracing::instrument(skip_all)]
    pub async fn active_worker_count(&self) -> anyhow::Result<usize> {
        self.task_store.read().await.active_worker_count().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn routing(&self) -> anyhow::Result<Vec<crate::TaskRouting>> {
        self.task_store.read().await.routing().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_task(&self, id: i64) -> anyhow::Result<Option<crate::Task>> {
        self.task_store.read().await.find_by_id(id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn load_all_tasks(&self, include_archived: bool) -> anyhow::Result<Vec<crate::Task>> {
        let store = self.task_store.read().await;
        if include_archived {
            store.load_all_with_archived().await
        } else {
            store.load_all().await
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn write_task(&self, task: &crate::Task) -> anyhow::Result<()> {
        self.task_store.write().await.write_task(task).await?;
        self.drain_pending_lifecycle_effects().await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_task(
        &self,
        id: i64,
        updates: crate::UpdateTaskInput,
    ) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::update_task(&store, id, updates).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_sessions_for_task(
        &self,
        id: i64,
    ) -> anyhow::Result<Vec<sessions_db::SessionRow>> {
        self.task_store
            .read()
            .await
            .list_sessions_for_task(id)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn close_pr(&self, repo: &str, pr_num: &str) -> anyhow::Result<()> {
        global_github::close_pr(repo, pr_num).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_tasks(
        &self,
        config: &settings::Config,
        ids: &[i64],
        close_pr: bool,
        force: bool,
    ) -> anyhow::Result<Vec<String>> {
        let opts = crate::io::task_cleanup::CleanupOptions { close_pr, force };
        let store = self.task_store.read().await;
        crate::runtime::dashboard::delete_tasks(config, &store, ids, &opts).await
    }

    pub fn load_health_state(&self) -> anyhow::Result<crate::io::health_store::HealthState> {
        let health_path = crate::config::worker_health_path();
        crate::io::health_store::load_health_state(&health_path)
    }

    #[tracing::instrument(skip_all)]
    pub async fn load_health_state_async(
        &self,
    ) -> anyhow::Result<crate::io::health_store::HealthState> {
        let health_path = crate::config::worker_health_path();
        crate::io::health_store::load_health_state_async(&health_path).await
    }

    pub fn health_counter(
        &self,
        state: &crate::io::health_store::HealthState,
        worker_name: &str,
        field: &str,
    ) -> u32 {
        crate::io::health_store::get_health_u32(state, worker_name, field)
    }

    #[tracing::instrument(skip_all)]
    pub async fn prepare_terminal_workbench(
        &self,
        project_name: &str,
        cwd: &str,
        is_resume: bool,
    ) -> anyhow::Result<Option<i64>> {
        workbench_runtime::prepare_terminal_workbench(self, project_name, cwd, is_resume).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn rollback_terminal_workbench(&self, workbench_id: i64) {
        workbench_runtime::rollback_terminal_workbench(self, workbench_id).await;
    }

    #[tracing::instrument(skip_all)]
    pub async fn record_terminal_cc_session(
        &self,
        cwd: &str,
        cc_session_id: &str,
    ) -> anyhow::Result<()> {
        workbench_runtime::record_terminal_cc_session(self, cwd, cc_session_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn notify_terminal_activity(&self, cwd: &str) -> anyhow::Result<bool> {
        workbench_runtime::notify_terminal_activity(self, cwd).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn broadcast_task_update(&self, id: i64) {
        let updated = {
            let store = self.task_store.read().await;
            match store.find_by_id(id).await {
                Ok(Some(task)) => Some(serde_json::to_value(&task).unwrap_or_default()),
                Ok(None) => {
                    tracing::warn!(
                        module = "captain-runtime-daemon",
                        task_id = id,
                        "broadcast skipped -- task not found"
                    );
                    return;
                }
                Err(err) => {
                    tracing::warn!(module = "captain-runtime-daemon", task_id = id, error = %err, "broadcast skipped -- DB read failed");
                    return;
                }
            }
        };
        let item: Option<api_types::TaskItem> =
            updated.and_then(|v| serde_json::from_value(v).ok());
        self.bus.send(global_bus::BusPayload::Tasks(Some(
            api_types::TaskEventData {
                action: Some("updated".into()),
                item,
                id: Some(id),
                cleared_by: None,
            },
        )));
    }

    #[tracing::instrument(skip_all)]
    pub async fn touch_workbench_activity(&self, workbench_id: i64) {
        let touched = match crate::io::queries::workbenches::touch_activity(
            &self.pool,
            workbench_id,
        )
        .await
        {
            Ok(touched) => touched,
            Err(err) => {
                tracing::warn!(module = "captain-runtime-daemon", workbench_id, error = %err, "failed to touch workbench activity");
                return;
            }
        };
        if touched {
            match crate::io::queries::workbenches::find_by_id(&self.pool, workbench_id).await {
                Ok(Some(updated)) => {
                    match crate::runtime::daemon::workbench_runtime::to_wire_workbench_item(
                        &updated,
                    ) {
                        Ok(item) => {
                            self.bus.send(global_bus::BusPayload::Workbenches(Some(
                                api_types::WorkbenchEventData {
                                    action: Some("updated".into()),
                                    item: Some(item),
                                },
                            )));
                        }
                        Err(e) => {
                            // Fire-and-forget caller — we can't propagate,
                            // but DO NOT emit an `item: None` event. The
                            // DB already committed; the frontend will
                            // resync on its next poll / reconnect.
                            tracing::error!(
                                module = "captain-runtime-daemon",
                                workbench_id,
                                error = %e,
                                "skipping workbench bus broadcast — serde failure indicates api-types schema drift"
                            );
                        }
                    }
                }
                Ok(None) => {
                    tracing::warn!(
                        module = "captain-runtime-daemon",
                        workbench_id,
                        "workbench not found after activity touch"
                    )
                }
                Err(err) => {
                    tracing::warn!(module = "captain-runtime-daemon", workbench_id, error = %err, "failed to load workbench after touch")
                }
            }
        }
    }

    pub fn resolve_task_cwd(&self, item: &crate::Task) -> anyhow::Result<PathBuf> {
        // Fail-fast: no fallback. Running an ask/advisor session inside
        // the wrong directory (previously: `first_project_path`, i.e.
        // whichever project hashes first) is worse than a clean error.
        // The caller turns this into a 4xx the user can act on by
        // reopening the task, which recovers the worktree via spawn's
        // WorktreePlan::Recreate path.
        let Some(stored) = item
            .worktree
            .as_deref()
            .map(global_infra::paths::expand_tilde)
        else {
            anyhow::bail!("task {} has no worktree assigned", item.id);
        };
        if !stored.is_dir() {
            anyhow::bail!(
                "task {} worktree missing on disk: {} — reopen the task to recover",
                item.id,
                stored.display()
            );
        }
        Ok(stored)
    }
}

pub(crate) fn degraded_failure_threshold() -> u32 {
    DEGRADED_FAILURE_THRESHOLD
}
