use std::path::PathBuf;

use api_types::TaskCreateResponse;
use global_db::lifecycle::LifecycleEffect;
use serde_json::Value;
use sessions_db::SessionRow;

use crate::types::EffectRequest;

use super::CaptainRuntime;

impl CaptainRuntime {
    pub fn worktrees_dir(&self) -> PathBuf {
        global_git::worktrees_dir()
    }

    #[tracing::instrument(skip_all)]
    pub async fn load_task(&self, id: i64) -> anyhow::Result<Option<crate::Task>> {
        self.find_task(id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn task_json(&self, id: i64) -> anyhow::Result<Option<api_types::TaskItem>> {
        self.load_task(id)
            .await?
            .map(|task| serde_json::to_value(task).and_then(serde_json::from_value))
            .transpose()
            .map_err(anyhow::Error::from)
    }

    #[tracing::instrument(skip_all)]
    pub async fn append_task_images(&self, id: i64, new_images: &[String]) -> anyhow::Result<()> {
        if new_images.is_empty() {
            return Ok(());
        }
        let joined = new_images.join(",");
        self.task_store
            .read()
            .await
            .update(id, |task| {
                task.images = Some(match task.images.take() {
                    Some(existing) if !existing.is_empty() => format!("{existing},{joined}"),
                    _ => joined.clone(),
                });
            })
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn enqueue_task_effects(
        &self,
        task_id: i64,
        cause: Option<&str>,
        effects: Vec<EffectRequest>,
    ) -> anyhow::Result<()> {
        let payloads: Vec<Value> = effects.iter().map(|e| e.into_payload()).collect();
        let refs: Vec<LifecycleEffect<'_>> = effects
            .iter()
            .zip(payloads.iter())
            .map(|(e, payload)| LifecycleEffect {
                effect_kind: e.into_effect_kind(),
                payload,
            })
            .collect();
        crate::io::queries::tasks_persist::enqueue_task_effects(
            &self.pool, task_id, "gateway", cause, refs,
        )
        .await?;
        self.drain_pending_lifecycle_effects().await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn persist_task_transition_with_effects(
        &self,
        task: &crate::Task,
        expected_status: &str,
        event: &crate::TimelineEvent,
        effects: Vec<EffectRequest>,
    ) -> anyhow::Result<bool> {
        let payloads: Vec<Value> = effects.iter().map(|e| e.into_payload()).collect();
        let refs: Vec<LifecycleEffect<'_>> = effects
            .iter()
            .zip(payloads.iter())
            .map(|(e, payload)| LifecycleEffect {
                effect_kind: e.into_effect_kind(),
                payload,
            })
            .collect();
        let command = crate::service::lifecycle::infer_transition_command(
            expected_status
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?,
            task.status,
        )?;
        let applied =
            crate::io::queries::tasks_persist::persist_status_transition_with_command_and_effects(
                &self.pool,
                task,
                expected_status,
                command,
                event,
                refs,
            )
            .await?;
        self.drain_pending_lifecycle_effects().await?;
        Ok(applied)
    }

    #[tracing::instrument(skip_all)]
    pub async fn task_artifacts(&self, task_id: i64) -> anyhow::Result<Vec<crate::TaskArtifact>> {
        crate::io::queries::artifacts::list_for_task(&self.pool, task_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn task_timeline(&self, id: &str) -> anyhow::Result<Vec<crate::TimelineEvent>> {
        crate::runtime::dashboard_timeline::get_item_timeline(id, None, &self.pool).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_task_sessions(&self, task_id: i64) -> anyhow::Result<Vec<SessionRow>> {
        self.list_sessions_for_task(task_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn fetch_pr_body(&self, repo: &str, pr_number: u32) -> anyhow::Result<String> {
        global_github::get_pr_body(repo, pr_number).await
    }

    pub fn append_task_note(
        &self,
        existing: Option<&str>,
        tag: &str,
        text: &str,
    ) -> Option<String> {
        crate::runtime::task_notes::append_tagged_note(existing, tag, text)
    }

    pub fn ambient_rate_limit_remaining_secs(&self) -> u64 {
        crate::runtime::ambient_rate_limit::remaining_secs()
    }

    #[tracing::instrument(skip_all)]
    pub async fn emit_task_timeline_event(
        &self,
        item: &crate::Task,
        summary: &str,
        data: crate::TimelineEventPayload,
    ) -> anyhow::Result<()> {
        crate::runtime::timeline_emit::emit_for_task(item, summary, data, &self.pool).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn reopen_item_from_human(
        &self,
        item: &mut crate::Task,
        feedback: &str,
        workflow: &settings::CaptainWorkflow,
        notifier: &crate::runtime::notify::Notifier,
    ) -> anyhow::Result<crate::runtime::action_contract::ReopenOutcome> {
        let config = self.settings.load_config();
        crate::runtime::action_contract::reopen_item(
            item, "human", feedback, &config, workflow, notifier, &self.pool, true,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn nudge_item(
        &self,
        item: &mut crate::Task,
        message: Option<&str>,
        workflow: &settings::CaptainWorkflow,
        notifier: &crate::runtime::notify::Notifier,
        alerts: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let config = self.settings.load_config();
        crate::runtime::action_contract::nudge_item(
            item, message, None, &config, workflow, notifier, alerts, &self.pool,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn answer_and_reclarify(
        &self,
        item: &crate::Task,
        answer: &str,
        workflow: &settings::CaptainWorkflow,
    ) -> anyhow::Result<crate::runtime::clarifier::ClarifierResult> {
        let config = self.settings.load_config();
        crate::runtime::clarifier_reclarify::answer_and_reclarify(
            item, answer, workflow, &config, &self.pool,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn apply_clarifier_result(
        &self,
        item: &mut crate::Task,
        result: crate::runtime::clarifier::ClarifierResult,
        workflow: &settings::CaptainWorkflow,
    ) -> anyhow::Result<()> {
        let notifier = crate::runtime::notify::Notifier::new(self.bus.clone());
        let session_id = result
            .session_id
            .clone()
            .or_else(|| item.session_ids.clarifier.clone())
            .unwrap_or_default();
        crate::runtime::tick_clarify_apply::apply_clarifier_result(
            item,
            result,
            &session_id,
            &notifier,
            &workflow.agent.resource_limits,
            &self.pool,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_task(
        &self,
        title: &str,
        project: Option<&str>,
        source: Option<&str>,
        provider: api_types::TaskProvider,
        use_glm_worker: bool,
    ) -> anyhow::Result<TaskCreateResponse> {
        self.add_task_with_context(title, project, None, source, provider, use_glm_worker)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_task_with_context(
        &self,
        title: &str,
        project: Option<&str>,
        context: Option<&str>,
        source: Option<&str>,
        provider: api_types::TaskProvider,
        use_glm_worker: bool,
    ) -> anyhow::Result<TaskCreateResponse> {
        let config = self.settings.load_config();
        // Run the git fetch + worktree creation half WITHOUT holding the
        // task_store read lock — a slow remote would otherwise stall the
        // captain tick's `task_store.write()` writers. Only acquire the
        // lock for the optional context-update step that follows.
        let value = crate::runtime::dashboard::add_task(
            &config,
            &self.pool,
            title,
            project,
            source,
            provider,
            use_glm_worker,
        )
        .await?;
        if let Some(ctx) = context {
            let store = self.task_store.read().await;
            crate::runtime::dashboard::update_task(
                &store,
                value.id,
                crate::UpdateTaskInput {
                    context: Some(Some(ctx.to_string())),
                    ..Default::default()
                },
            )
            .await?;
        }
        self.drain_pending_lifecycle_effects().await?;
        Ok(value)
    }
}
