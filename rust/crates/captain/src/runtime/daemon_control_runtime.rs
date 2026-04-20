use serde_json::json;
use serde_json::Value;

use super::CaptainRuntime;

impl CaptainRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn health_summary_counts(&self) -> anyhow::Result<(usize, usize)> {
        let store = self.task_store.read().await;
        let active = store.active_worker_count().await?;
        let total = store.routing().await?.len();
        Ok((active, total))
    }

    #[tracing::instrument(skip_all)]
    pub async fn trigger_captain_tick(
        &self,
        workflow: &settings::config::CaptainWorkflow,
        dry_run: bool,
        emit_notifications: bool,
    ) -> anyhow::Result<crate::TickResult> {
        let config = self.settings.load_config();
        crate::runtime::dashboard::trigger_captain_tick(
            &config,
            workflow,
            dry_run,
            Some(&self.bus),
            emit_notifications,
            &self.task_store,
            &self.cancellation_token,
            &self.task_tracker,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn triage_pending_review(
        &self,
        item_id: Option<&str>,
    ) -> anyhow::Result<api_types::TriageResponse> {
        let config = self.settings.load_config();
        let store = self.task_store.read().await;
        crate::runtime::dashboard_triage::triage_pending_review(&config, &store, item_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn stop_all_workers(&self) -> anyhow::Result<u32> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::stop_all_workers(&store, &self.pool).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn probe_credential_usage(
        &self,
        row: &settings::CredentialRow,
    ) -> Result<settings::UsageSnapshot, settings::ProbeError> {
        crate::runtime::credential_usage_poll::probe_and_persist(&self.pool, row).await
    }

    pub fn resolve_worker_pid(&self, cc_session_id: &str, worker_name: &str) -> Option<u32> {
        crate::io::pid_lookup::resolve_pid(cc_session_id, worker_name).map(|pid| pid.as_u32())
    }

    #[tracing::instrument(skip_all)]
    pub async fn kill_worker_process(&self, pid: u32) -> anyhow::Result<()> {
        crate::io::process_manager::kill_worker_process(crate::Pid::new(pid)).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn workers_dashboard(
        &self,
        workflow: &settings::config::CaptainWorkflow,
    ) -> anyhow::Result<Vec<Value>> {
        let all_items = self.load_all_tasks(false).await?;
        let health_path = crate::config::worker_health_path();
        let health = crate::io::health_store::load_health_state(&health_path)?;
        let nudge_budget = workflow.agent.max_interventions;
        let stale_threshold_s = workflow.agent.stale_threshold_s.as_secs_f64();

        Ok(all_items
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    crate::ItemStatus::InProgress
                        | crate::ItemStatus::CaptainReviewing
                        | crate::ItemStatus::CaptainMerging
                ) && task.worker.is_some()
            })
            .map(|task| {
                let worker_name = task.worker.as_deref().unwrap_or("");
                let nudge_count =
                    crate::io::health_store::get_health_u32(&health, worker_name, "nudge_count");
                let cc_sid = task.session_ids.worker.as_deref().unwrap_or("");
                let pid = self.resolve_worker_pid(cc_sid, worker_name);
                let last_action = health
                    .get(worker_name)
                    .and_then(|v| v.get("last_action"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let stream_stale_s = task
                    .session_ids
                    .worker
                    .as_deref()
                    .map(global_infra::paths::stream_path_for_session)
                    .and_then(|path: std::path::PathBuf| {
                        global_claude::stream_stale_seconds(path.as_path())
                    });
                let process_alive = pid
                    .map(crate::Pid::new)
                    .is_some_and(global_claude::is_process_alive);
                let in_captain_phase = matches!(
                    task.status,
                    crate::ItemStatus::CaptainReviewing | crate::ItemStatus::CaptainMerging
                );
                let is_stale = if in_captain_phase {
                    false
                } else {
                    match (process_alive, stream_stale_s) {
                        (true, Some(s)) => s >= stale_threshold_s,
                        (true, None) => false,
                        (false, _) => true,
                    }
                };
                json!({
                    "id": task.id,
                    "title": task.title,
                    "status": task.status.as_str(),
                    "worker": task.worker,
                    "project": task.project,
                    "github_repo": task.github_repo,
                    "worktree": task.worktree,
                    "branch": task.branch,
                    "pr_number": task.pr_number,
                    "started_at": task.worker_started_at,
                    "last_activity_at": task.last_activity_at,
                    "cc_session_id": task.session_ids.worker,
                    "intervention_count": task.intervention_count,
                    "nudge_count": nudge_count,
                    "nudge_budget": nudge_budget,
                    "last_action": last_action,
                    "pid": pid,
                    "is_stale": is_stale,
                })
            })
            .collect())
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_worker_task(&self, id: &str) -> anyhow::Result<Option<crate::Task>> {
        let store = self.task_store.read().await;
        let routing = store.routing().await?;
        if let Some(idx) = routing
            .iter()
            .find(|idx| idx.worker.as_deref() == Some(id) || idx.id.to_string() == id)
        {
            return store.find_by_id(idx.id).await;
        }
        Ok(store
            .load_all()
            .await?
            .into_iter()
            .find(|task| task.session_ids.worker.as_deref() == Some(id)))
    }
}
