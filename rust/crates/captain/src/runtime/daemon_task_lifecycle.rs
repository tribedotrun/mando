//! Pass-through lifecycle methods on `CaptainRuntime` that wrap
//! `runtime::dashboard` calls and drain any pending lifecycle effects.
//!
//! Split out of `daemon_task_runtime.rs` to keep file length under the 500-line
//! cap (`mando-dev check rust`).

use super::CaptainRuntime;

impl CaptainRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn bulk_update_tasks(
        &self,
        ids: &[i64],
        updates: crate::UpdateTaskInput,
    ) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::bulk_update_tasks(&store, ids, updates, &self.pool).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn queue_item(&self, id: i64, reason: &str) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::queue_item(&store, id, reason).await?;
        self.drain_pending_lifecycle_effects().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_tasks_for_api(
        &self,
        ids: &[i64],
        close_pr: bool,
        force: bool,
    ) -> anyhow::Result<Vec<String>> {
        let config = self.settings.load_config();
        let opts = crate::io::task_cleanup::CleanupOptions { close_pr, force };
        let store = self.task_store.read().await;
        crate::runtime::dashboard::delete_tasks(&config, &store, ids, &opts).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn merge_pr(
        &self,
        pr_number: i64,
        project: &str,
    ) -> anyhow::Result<api_types::MergeResponse> {
        let store = self.task_store.read().await;
        let value = crate::runtime::dashboard::merge_pr(&store, pr_number, project).await?;
        self.drain_pending_lifecycle_effects().await?;
        Ok(value)
    }

    #[tracing::instrument(skip_all)]
    pub async fn accept_item(&self, id: i64) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::accept_item(&store, id).await?;
        self.drain_pending_lifecycle_effects().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn cancel_item(&self, id: i64) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::cancel_item(&store, id, &self.pool).await?;
        self.drain_pending_lifecycle_effects().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn rework_item(&self, id: i64, feedback: &str) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::rework_item(&store, id, feedback).await?;
        self.drain_pending_lifecycle_effects().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn retry_item(&self, id: i64) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::retry_item(&store, id).await?;
        self.drain_pending_lifecycle_effects().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn validate_rate_limited_task(&self, id: i64) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::validate_rate_limited_task(&store, id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn handoff_item(&self, id: i64) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::handoff_item(&store, id, &self.pool).await?;
        self.drain_pending_lifecycle_effects().await
    }
}
