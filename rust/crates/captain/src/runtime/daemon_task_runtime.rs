use std::path::PathBuf;

use api_types::TaskCreateResponse;
use global_db::lifecycle::LifecycleEffect;
use serde_json::Value;
use sessions_db::SessionRow;
use settings::CaptainWorkflow;

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
    pub async fn set_task_planning(&self, id: i64, planning: bool) -> anyhow::Result<()> {
        let store = self.task_store.read().await;
        crate::runtime::dashboard::set_task_planning(&store, id, planning).await
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
            task.planning,
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
    pub async fn set_task_ask_session(
        &self,
        task_id: i64,
        session_id: Option<String>,
    ) -> anyhow::Result<()> {
        match self.load_task(task_id).await? {
            Some(mut task) => {
                task.session_ids.ask = session_id;
                self.write_task(&task).await?;
            }
            None => tracing::warn!(
                module = "captain-runtime-daemon_task_runtime",
                task_id,
                "task vanished while updating ask session id"
            ),
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn set_task_advisor_session(
        &self,
        task_id: i64,
        session_id: Option<String>,
    ) -> anyhow::Result<()> {
        match self.load_task(task_id).await? {
            Some(mut task) => {
                task.session_ids.advisor = session_id;
                self.write_task(&task).await?;
            }
            None => tracing::warn!(
                module = "captain-runtime-daemon_task_runtime",
                task_id,
                "task vanished while updating advisor session id"
            ),
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn task_artifacts(&self, task_id: i64) -> anyhow::Result<Vec<crate::TaskArtifact>> {
        crate::io::queries::artifacts::list_for_task(&self.pool, task_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn task_ask_history(
        &self,
        task_id: i64,
    ) -> anyhow::Result<Vec<crate::AskHistoryEntry>> {
        crate::io::queries::ask_history::load(&self.pool, task_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn latest_clarifier_questions(
        &self,
        task_id: i64,
    ) -> anyhow::Result<Option<Vec<api_types::ClarifierQuestionPayload>>> {
        crate::io::queries::timeline::latest_clarifier_questions(&self.pool, task_id).await
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

    #[tracing::instrument(skip_all)]
    pub async fn build_task_timeline_text(&self, task_id: i64) -> anyhow::Result<String> {
        crate::runtime::task_ask::build_timeline_text(&self.pool, task_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn build_task_ask_initial_prompt(
        &self,
        item: &crate::Task,
        question: &str,
        workflow: &CaptainWorkflow,
    ) -> anyhow::Result<String> {
        let timeline_text = self.build_task_timeline_text(item.id).await?;
        crate::runtime::task_ask::build_initial_prompt(
            item,
            &item.id.to_string(),
            question,
            workflow,
            &timeline_text,
        )
    }

    #[tracing::instrument(skip_all)]
    pub async fn persist_task_question(
        &self,
        task_id: i64,
        ask_id: &str,
        session_id: &str,
        question: &str,
    ) -> anyhow::Result<()> {
        crate::runtime::task_ask::persist_question(
            &self.pool, task_id, ask_id, session_id, question,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn persist_task_answer(
        &self,
        task_id: i64,
        ask_id: &str,
        session_id: &str,
        question: &str,
        answer: &str,
        intent: &str,
    ) -> anyhow::Result<()> {
        crate::runtime::task_ask::persist_answer(
            &self.pool, task_id, ask_id, session_id, question, answer, intent,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn persist_task_error(
        &self,
        task_id: i64,
        ask_id: &str,
        session_id: &str,
        question: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        crate::runtime::task_ask::persist_error(
            &self.pool, task_id, ask_id, session_id, question, error,
        )
        .await
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
    ) -> anyhow::Result<TaskCreateResponse> {
        self.add_task_with_context(title, project, None, source)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_task_with_context(
        &self,
        title: &str,
        project: Option<&str>,
        context: Option<&str>,
        source: Option<&str>,
    ) -> anyhow::Result<TaskCreateResponse> {
        let config = self.settings.load_config();
        let store = self.task_store.read().await;
        let value = crate::runtime::dashboard::add_task_with_context(
            &config, &store, title, project, context, source,
        )
        .await?;
        drop(store);
        self.drain_pending_lifecycle_effects().await?;
        Ok(value)
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_task_workbench(
        &self,
        task_id: i64,
        project_name: &str,
        title: &str,
    ) -> anyhow::Result<()> {
        let project_row = settings::projects::resolve(&self.pool, project_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;
        let project_path = global_infra::paths::expand_tilde(&project_row.path);

        let suffix = format!("todo-{task_id}");
        let branch = format!("mando/{suffix}");
        let wt_path = global_git::worktree_path(&project_path, &suffix);

        global_git::fetch_origin(&project_path).await?;
        let default_branch = global_git::default_branch(&project_path).await?;
        if wt_path.exists() {
            global_infra::best_effort!(
                global_git::remove_worktree(&project_path, &wt_path).await,
                "daemon_task_runtime: global_git::remove_worktree(&project_path, &wt_path).awa"
            );
        }
        global_infra::best_effort!(
            global_git::delete_local_branch(&project_path, &branch).await,
            "daemon_task_runtime: global_git::delete_local_branch(&project_path, &branch)."
        );
        global_git::create_worktree(&project_path, &branch, &wt_path, &default_branch).await?;
        crate::io::worktree_bootstrap::copy_local_files(&project_path, &wt_path).await;

        let workbench = crate::Workbench::new(
            project_row.id,
            project_row.name.clone(),
            wt_path.to_string_lossy().to_string(),
            title.to_string(),
        );
        let workbench_id = crate::io::queries::workbenches::insert(&self.pool, &workbench).await?;
        let patch = crate::UpdateTaskInput {
            workbench_id: Some(workbench_id),
            ..Default::default()
        };
        self.update_task(task_id, patch).await?;
        self.bus.send(global_bus::BusPayload::Workbenches(None));
        Ok(())
    }
}
