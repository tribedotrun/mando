use anyhow::Result;
use settings::{CaptainWorkflow, ProjectConfig, StageAgentConfig};

use crate::Task;

pub(crate) fn implementation_provider(
    item: &Task,
    workflow: &CaptainWorkflow,
) -> global_types::ExecutionAdapter {
    if !item.use_glm_worker {
        return item.provider;
    }
    workflow.stages.implementation.adapter
}

#[tracing::instrument(skip(pool, item), fields(task_id = item.id))]
pub(crate) async fn persisted_worker_provider(
    pool: &sqlx::SqlitePool,
    item: &Task,
) -> global_types::ExecutionAdapter {
    let Some(session_id) = item
        .session_ids
        .worker
        .as_deref()
        .filter(|sid| !sid.is_empty())
    else {
        return item.provider;
    };

    persisted_session_provider(pool, session_id, item.provider).await
}

#[tracing::instrument(skip(pool), fields(session_id, fallback_provider = %fallback.as_str()))]
pub(crate) async fn persisted_session_provider(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    fallback: global_types::ExecutionAdapter,
) -> global_types::ExecutionAdapter {
    match sessions_db::session_by_id(pool, session_id).await {
        Ok(Some(row)) => row.provider,
        Ok(None) => {
            tracing::warn!(
                module = "agent_runtime",
                session_id,
                fallback_provider = %fallback.as_str(),
                "session row missing while resolving persisted provider; using fallback"
            );
            fallback
        }
        Err(e) => {
            tracing::warn!(
                module = "agent_runtime",
                session_id,
                error = %e,
                fallback_provider = %fallback.as_str(),
                "failed to load session row while resolving persisted provider; using fallback"
            );
            fallback
        }
    }
}

#[tracing::instrument(skip(pool), fields(session_id))]
pub(crate) async fn persisted_session_model(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Option<String> {
    match sessions_db::session_by_id(pool, session_id).await {
        Ok(Some(row)) => usable_persisted_session_model(row.provider, &row.model),
        Ok(None) => {
            tracing::warn!(
                module = "agent_runtime",
                session_id,
                "session row missing while resolving persisted model"
            );
            None
        }
        Err(e) => {
            tracing::warn!(
                module = "agent_runtime",
                session_id,
                error = %e,
                "failed to load session row while resolving persisted model"
            );
            None
        }
    }
}

fn usable_persisted_session_model(
    provider: global_types::ExecutionAdapter,
    model: &str,
) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    if provider == global_types::ExecutionAdapter::Claude && trimmed == "default" {
        return None;
    }
    Some(trimmed.to_string())
}

fn implementation_stage_applies<'a>(
    item: &Task,
    workflow: &'a CaptainWorkflow,
    provider: global_types::ExecutionAdapter,
) -> Option<&'a StageAgentConfig> {
    item.use_glm_worker
        .then_some(&workflow.stages.implementation)
        .filter(|stage| stage.adapter == provider)
}

pub(super) fn implementation_worker_model(
    item: &Task,
    workflow: &CaptainWorkflow,
    provider: global_types::ExecutionAdapter,
    legacy_model: &str,
    persisted_model: Option<&str>,
) -> String {
    if let Some(model) = persisted_model {
        return model.to_string();
    }
    implementation_stage_applies(item, workflow, provider)
        .map(|stage| stage.model.clone())
        .unwrap_or_else(|| legacy_model.to_string())
}

/// Model a Claude rebase worker runs on: the same resolution the initial
/// worker spawn uses, so a task pinned to a non-default implementation model
/// keeps it. No persisted model — rebase mints a fresh session id. Provider is
/// pinned to Claude because only the Claude owner branch in the rebase phase
/// runner consumes this model.
pub(crate) fn claude_rebase_worker_model(item: &Task, workflow: &CaptainWorkflow) -> String {
    implementation_worker_model(
        item,
        workflow,
        global_types::ExecutionAdapter::Claude,
        &workflow.models.worker,
        None,
    )
}

pub(super) fn codex_agent_config_for_worker(
    item: &Task,
    workflow: &CaptainWorkflow,
    persisted_model: Option<&str>,
) -> settings::AgentConfig {
    let stage = implementation_stage_applies(item, workflow, global_types::ExecutionAdapter::Codex);
    let model = persisted_model
        .map(str::to_string)
        .or_else(|| stage.map(|stage_config| stage_config.model.clone()));
    let Some(model) = model else {
        return workflow.agent.clone();
    };

    let mut agent_config = workflow.agent.clone();
    let mut codex_config = agent_config
        .codex
        .clone()
        .unwrap_or(settings::CodexAgentConfig {
            model: None,
            reasoning_effort: None,
            service_tier: None,
        });
    codex_config.model = Some(model);
    agent_config.codex = Some(codex_config);
    if let Some(stage_config) = stage {
        agent_config.ops_timeout_s = stage_config.session_start_timeout_s;
    }
    agent_config
}

/// Provider-neutral worker spawn boundary.
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub async fn spawn_worker(
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<super::spawner::SpawnResult> {
    let provider = implementation_provider(item, workflow);
    super::agent_runtime::Adapter::new(provider)
        .start_worker(project_config, item, workflow, pool)
        .await
}

#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id))]
pub(super) async fn spawn_claude_worker(
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    worker_model: &str,
) -> Result<super::spawner::SpawnResult> {
    let claude_path = global_claude::resolve_claude_binary();
    if !claude_path.exists() && claude_path.to_str() == Some("claude") {
        let found = tokio::process::Command::new("which")
            .arg("claude")
            .output()
            .await
            .map(|out| out.status.success())
            .unwrap_or(false);
        if !found {
            anyhow::bail!(
                "claude binary not found (checked {:?} and PATH)",
                claude_path
            );
        }
    }

    let credential = super::tick_spawn::pick_credential(pool).await;
    if credential.is_none() {
        if let Ok(true) = settings::credentials::has_any(pool).await {
            let remaining = settings::credentials::earliest_cooldown_remaining_secs(pool)
                .await
                .unwrap_or(600);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let earliest_reset = now + remaining;
            if let Err(e) =
                crate::io::queries::tasks::set_paused_until(pool, item.id, earliest_reset).await
            {
                tracing::warn!(
                    module = "spawner",
                    task_id = item.id,
                    error = %e,
                    "failed to pause task after all credentials exhausted"
                );
            }
            tracing::warn!(
                module = "spawner",
                task_id = item.id,
                earliest_reset,
                "paused worker dispatch — every credential in pool is rate-limited"
            );
            anyhow::bail!("all credentials rate-limited; task paused until {earliest_reset}");
        }
    }
    let worker_cred = credential
        .as_ref()
        .map(|c| super::spawner::WorkerCredential {
            id: c.0,
            token: &c.1,
        });

    super::spawner::spawn_worker(
        item,
        project_config,
        workflow,
        pool,
        worker_cred.as_ref(),
        worker_model,
    )
    .await
}

pub(crate) struct AgentOutputSchema(pub(crate) serde_json::Value);

pub(crate) struct AgentStructuredSession {
    pub(crate) session_id: String,
}

pub(crate) struct AgentWorkerResume {
    pub(crate) pid: crate::Pid,
    pub(crate) session_id: String,
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(pool, prompt, output_schema), fields(provider = %provider.as_str(), task_id, caller))]
pub(crate) async fn spawn_structured_session(
    provider: global_types::ExecutionAdapter,
    pool: &sqlx::SqlitePool,
    caller: &str,
    task_id: i64,
    project: &str,
    worker_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    output_schema: AgentOutputSchema,
    resume_thread_id: Option<&str>,
    agent_config: &settings::AgentConfig,
) -> Result<AgentStructuredSession> {
    super::agent_runtime::Adapter::new(provider)
        .start_structured(
            pool,
            caller,
            task_id,
            project,
            worker_name,
            cwd,
            prompt,
            output_schema,
            resume_thread_id,
            agent_config,
        )
        .await
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(pool, item, prompt), fields(provider = %item.provider.as_str(), task_id = item.id, session_id))]
pub(crate) async fn resume_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    worker_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    session_id: &str,
    model: &str,
    workflow: &CaptainWorkflow,
) -> Result<AgentWorkerResume> {
    let fallback_provider = implementation_provider(item, workflow);
    let provider = persisted_session_provider(pool, session_id, fallback_provider).await;
    let persisted_model = persisted_session_model(pool, session_id).await;
    super::agent_runtime::Adapter::new(provider)
        .resume_worker(
            pool,
            item,
            worker_name,
            cwd,
            prompt,
            session_id,
            model,
            workflow,
            persisted_model.as_deref(),
        )
        .await
}

pub(crate) fn worker_resume_replacement_reason(
    provider: global_types::ExecutionAdapter,
    stream_path: &std::path::Path,
    workflow: &CaptainWorkflow,
) -> Option<String> {
    super::agent_runtime::Adapter::new(provider).resume_replacement_reason(stream_path, workflow)
}

pub(crate) fn uses_shared_process(provider: global_types::ExecutionAdapter) -> bool {
    super::agent_runtime::Adapter::new(provider).uses_shared_process()
}

#[tracing::instrument(skip_all, fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn interrupt_session_before_kill(
    provider: global_types::ExecutionAdapter,
    session_id: &str,
) -> Result<()> {
    super::agent_runtime::Adapter::new(provider)
        .interrupt(session_id)
        .await
}

#[tracing::instrument(skip_all, fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn terminate_worker_process(
    provider: global_types::ExecutionAdapter,
    session_id: &str,
) -> Result<()> {
    super::agent_runtime::Adapter::new(provider)
        .terminate(session_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_types::{SessionStatus, TaskProvider};
    use sessions_db::{upsert_session, SessionUpsert};

    async fn test_pool() -> sqlx::SqlitePool {
        global_db::Db::open_in_memory()
            .await
            .unwrap()
            .pool()
            .clone()
    }

    async fn insert_session_with_model(
        pool: &sqlx::SqlitePool,
        provider: TaskProvider,
        session_id: &str,
        model: &str,
    ) {
        upsert_session(
            pool,
            &SessionUpsert {
                provider,
                session_id,
                created_at: "2026-06-28T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model,
                status: SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(1),
                scout_item_id: None,
                worker_name: Some("worker-1"),
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        .unwrap();
    }

    async fn insert_session(pool: &sqlx::SqlitePool, provider: TaskProvider, session_id: &str) {
        insert_session_with_model(pool, provider, session_id, "test-model").await;
    }

    fn workflow_with_implementation_stage(adapter: &str, model: &str) -> CaptainWorkflow {
        let yaml = format!(
            r#"
stages:
  implementation:
    adapter: "{adapter}"
    model: "{model}"
    session_start_timeout_s: 9
    variant: null
"#
        );
        settings::parse_captain_workflow_or_default(
            Some(&yaml),
            std::path::Path::new("test-captain-workflow.yaml"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn persisted_worker_provider_reads_actual_session_provider() {
        let pool = test_pool().await;
        let session_id = "worker-provider-from-session-row";
        insert_session(&pool, TaskProvider::Codex, session_id).await;

        let mut item = Task::new("worker");
        item.provider = TaskProvider::Claude;
        item.use_glm_worker = true;
        item.session_ids.worker = Some(session_id.to_string());

        assert_eq!(
            persisted_worker_provider(&pool, &item).await,
            TaskProvider::Codex
        );
    }

    #[tokio::test]
    async fn persisted_session_model_reads_actual_session_model() {
        let pool = test_pool().await;
        let session_id = "worker-model-from-session-row";
        insert_session(&pool, TaskProvider::Codex, session_id).await;

        assert_eq!(
            persisted_session_model(&pool, session_id).await.as_deref(),
            Some("test-model")
        );
    }

    #[tokio::test]
    async fn persisted_session_model_ignores_empty_model() {
        let pool = test_pool().await;
        let session_id = "worker-model-empty-session-row";
        insert_session_with_model(&pool, TaskProvider::Codex, session_id, "   ").await;

        assert_eq!(persisted_session_model(&pool, session_id).await, None);
    }

    #[tokio::test]
    async fn persisted_session_model_ignores_claude_default_sentinel() {
        let pool = test_pool().await;
        let session_id = "worker-model-default-session-row";
        insert_session_with_model(&pool, TaskProvider::Claude, session_id, "default").await;

        assert_eq!(persisted_session_model(&pool, session_id).await, None);
    }

    #[test]
    fn stage_model_applies_to_codex_implementation_worker() {
        let workflow = workflow_with_implementation_stage("codex", "stage-codex-model");
        let mut item = Task::new("worker");
        item.use_glm_worker = true;

        let model = implementation_worker_model(
            &item,
            &workflow,
            TaskProvider::Codex.into(),
            "legacy-worker-model",
            None,
        );
        let agent_config = codex_agent_config_for_worker(&item, &workflow, None);

        assert_eq!(model, "stage-codex-model");
        assert_eq!(
            agent_config.codex.and_then(|codex| codex.model).as_deref(),
            Some("stage-codex-model")
        );
        assert_eq!(
            agent_config.ops_timeout_s,
            std::time::Duration::from_secs(9)
        );
    }

    /// Rebase must land on the same implementation model the initial worker
    /// spawn resolved, not the raw `workflow.models.worker` default.
    #[test]
    fn rebase_worker_reuses_the_implementation_stage_model() {
        let workflow = workflow_with_implementation_stage("claude", "stage-claude-model");
        let mut item = Task::new("worker");
        item.provider = TaskProvider::Claude;
        item.use_glm_worker = true;

        assert_eq!(
            claude_rebase_worker_model(&item, &workflow),
            "stage-claude-model"
        );
        assert_eq!(
            claude_rebase_worker_model(&item, &workflow),
            implementation_worker_model(
                &item,
                &workflow,
                TaskProvider::Claude.into(),
                &workflow.models.worker,
                None,
            ),
            "rebase must mirror the initial worker spawn's model resolution"
        );
    }

    /// A Codex implementation stage must not leak its model into a Claude
    /// rebase spawn.
    #[test]
    fn rebase_worker_ignores_a_non_claude_stage_model() {
        let workflow = workflow_with_implementation_stage("codex", "stage-codex-model");
        let mut item = Task::new("worker");
        item.provider = TaskProvider::Claude;
        item.use_glm_worker = true;

        assert_eq!(
            claude_rebase_worker_model(&item, &workflow),
            workflow.models.worker
        );
    }

    #[test]
    fn premium_opt_out_keeps_legacy_worker_model() {
        let workflow = workflow_with_implementation_stage("codex", "stage-codex-model");
        let mut item = Task::new("worker");
        item.use_glm_worker = false;

        let model = implementation_worker_model(
            &item,
            &workflow,
            TaskProvider::Codex.into(),
            "legacy-worker-model",
            None,
        );

        assert_eq!(model, "legacy-worker-model");
    }
}
