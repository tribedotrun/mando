use anyhow::Result;
use global_types::{ExecutionAdapter, TaskOwnerProvider};
use settings::{CaptainWorkflow, Config, ProjectConfig};

use crate::{Pid, Task};

pub(crate) use super::agent_liveness::{is_session_active, session_liveness, AgentLivenessStatus};
pub(crate) use super::agent_nudge::{nudge_worker, AgentNudgeOutcome};
pub(crate) use super::agent_session_result::{
    poll_structured_session_output, record_interrupted_result, session_output_text,
    should_record_interrupted_result, stream_meta_path, stream_path, AgentSessionOutput,
    AgentSessionPoll,
};

pub(crate) use super::agent_worker_runtime::{
    claude_rebase_worker_model, implementation_provider, interrupt_session_before_kill,
    persisted_session_provider, persisted_worker_provider, resume_worker, spawn_structured_session,
    spawn_worker, terminate_worker_process, uses_shared_process, worker_resume_replacement_reason,
    AgentOutputSchema, AgentStructuredSession, AgentWorkerResume,
};

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub(crate) async fn spawn_clarifier_session(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<Option<String>> {
    super::clarifier_session::spawn_initial(item, config, workflow, pool).await
}

pub(crate) fn spawn_detached_clarifier_session(
    provider: global_types::ExecutionAdapter,
    task: Task,
    workflow: CaptainWorkflow,
    config: Config,
    pool: sqlx::SqlitePool,
    session_id: String,
    task_tracker: &tokio_util::task::TaskTracker,
) {
    let owner = Adapter::new(provider).task_owner();
    if matches!(owner, Ok(global_types::TaskOwnerProvider::Claude)) {
        super::claude_clarifier_session::spawn_detached(
            task,
            workflow,
            config,
            pool,
            session_id,
            task_tracker,
        );
    } else {
        tracing::warn!(
            module = "agent_runtime",
            task_id = task.id,
            "asked to spawn detached Claude clarifier for a non-Claude task; ignoring"
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, trigger))]
pub(crate) async fn spawn_review_session(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    cwd: std::path::PathBuf,
    parsed_trigger: crate::ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    super::review_session::spawn(
        item,
        trigger,
        db_status,
        cwd,
        parsed_trigger,
        worker_contexts_text,
        workflow,
        notifier,
        pool,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, pr_number))]
pub(crate) async fn spawn_merge_session(
    item: &mut Task,
    cwd: &std::path::Path,
    notifier: &super::notify::Notifier,
    pool: &sqlx::SqlitePool,
    pr_url: &str,
    pr_number: &str,
    prompt: &str,
    workflow: &CaptainWorkflow,
) -> Result<()> {
    super::merge_session::spawn(
        item, cwd, notifier, pool, pr_url, pr_number, prompt, workflow,
    )
    .await
}

pub(crate) struct AgentRebaseSession {
    pub(crate) session_id: String,
    pub(crate) pid: crate::Pid,
}

#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, worker = session_name))]
pub(crate) async fn spawn_rebase_worker(
    item: &Task,
    pool: &sqlx::SqlitePool,
    session_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    model: &str,
    workflow: &CaptainWorkflow,
) -> Result<AgentRebaseSession> {
    let session =
        super::rebase_session::spawn(item, pool, session_name, cwd, prompt, model, workflow)
            .await?;
    Ok(AgentRebaseSession {
        session_id: session.session_id,
        pid: session.pid,
    })
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub(crate) async fn answer_and_reclarify_session(
    item: &Task,
    prompt: &str,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    prior_resume_sid: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<super::clarifier::ClarifierResult> {
    super::clarifier_session::answer_followup(item, prompt, cwd, workflow, prior_resume_sid, pool)
        .await
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id))]
pub(crate) async fn answer_claude_clarifier(
    item: &Task,
    prompt: &str,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    prior_resume_sid: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<super::clarifier::ClarifierResult> {
    super::claude_clarifier_session::answer_followup(
        item,
        prompt,
        cwd,
        workflow,
        prior_resume_sid,
        pool,
    )
    .await
}

#[tracing::instrument(skip(message), fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn steer(
    provider: global_types::ExecutionAdapter,
    session_id: &str,
    message: String,
) -> Result<bool> {
    Adapter::new(provider).steer(session_id, message).await
}

/// Opaque provider result consumed by the provider-neutral session poller.
pub(super) struct AdapterResult(pub(super) serde_json::Value);

pub(super) struct AdapterRebaseSession {
    pub(super) session_id: String,
    pub(super) pid: Pid,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct Adapter(ExecutionAdapter);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InactiveSessionPolicy {
    Process,
    Stream,
}

impl Adapter {
    pub(crate) fn new(adapter: impl Into<ExecutionAdapter>) -> Self {
        Self(adapter.into())
    }

    pub(crate) fn for_task(item: &Task) -> Result<Self> {
        let owner = TaskOwnerProvider::try_from(item.provider).map_err(anyhow::Error::msg)?;
        Ok(Self(owner.into()))
    }

    pub(crate) fn task_owner(self) -> Result<TaskOwnerProvider> {
        TaskOwnerProvider::try_from(self.0).map_err(anyhow::Error::msg)
    }

    pub(crate) fn is_claude(self) -> bool {
        self.0 == ExecutionAdapter::Claude
    }

    pub(crate) fn requires_structured_output(self) -> bool {
        !self.is_claude()
    }

    pub(crate) fn inactive_session_policy(self) -> InactiveSessionPolicy {
        match self.0 {
            ExecutionAdapter::Claude => InactiveSessionPolicy::Process,
            ExecutionAdapter::Codex | ExecutionAdapter::OpenCode => InactiveSessionPolicy::Stream,
        }
    }

    pub(crate) fn stream_path(self, session_id: &str) -> std::path::PathBuf {
        match self.0 {
            ExecutionAdapter::Claude => global_infra::paths::stream_path_for_session(session_id),
            ExecutionAdapter::Codex => {
                global_infra::paths::codex_derived_stream_path_for_session(session_id)
            }
            ExecutionAdapter::OpenCode => {
                global_infra::paths::opencode_stream_path_for_session(session_id)
            }
        }
    }

    pub(crate) fn stream_meta_path(self, session_id: &str) -> std::path::PathBuf {
        match self.0 {
            ExecutionAdapter::Claude => {
                global_infra::paths::stream_meta_path_for_session(session_id)
            }
            ExecutionAdapter::Codex => {
                global_infra::paths::codex_derived_stream_meta_path_for_session(session_id)
            }
            ExecutionAdapter::OpenCode => {
                global_infra::paths::opencode_stream_meta_path_for_session(session_id)
            }
        }
    }

    pub(crate) fn is_finished(self, session_id: &str) -> bool {
        match self.0 {
            ExecutionAdapter::Claude => global_claude::is_session_finished(session_id),
            ExecutionAdapter::Codex | ExecutionAdapter::OpenCode => {
                agent_runtime_core::is_stream_meta_finished_at(&self.stream_meta_path(session_id))
            }
        }
    }

    pub(crate) fn record_interrupted_result(self, stream_path: &std::path::Path) {
        if self.is_claude() {
            agent_runtime_core::write_interrupted_result(stream_path);
        }
    }

    pub(crate) fn should_record_interrupted_result(self, stream_path: &std::path::Path) -> bool {
        self.is_claude()
            && self.result(stream_path).is_none_or(|result| {
                agent_runtime_core::result_outcome(&result.0) != api_types::ResultOutcome::Success
            })
    }

    pub(super) fn result(self, stream_path: &std::path::Path) -> Option<AdapterResult> {
        agent_runtime_core::get_stream_result(stream_path).map(AdapterResult)
    }

    pub(crate) fn poll(self, session_id: &str) -> super::agent_session_result::AgentSessionPoll {
        super::agent_session_result::poll_for_adapter(self, session_id)
    }

    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), session_id))]
    pub(crate) async fn is_active(self, session_id: &str, pid: Pid) -> bool {
        match self.0 {
            ExecutionAdapter::Codex => super::codex_app_server::is_turn_active(session_id).await,
            ExecutionAdapter::Claude | ExecutionAdapter::OpenCode => {
                pid.as_u32() > 0 && agent_runtime_core::is_process_alive(pid)
            }
        }
    }

    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), task_id = item.id))]
    pub(crate) async fn start_worker(
        self,
        project_config: &ProjectConfig,
        item: &Task,
        workflow: &CaptainWorkflow,
        pool: &sqlx::SqlitePool,
    ) -> Result<super::spawner::SpawnResult> {
        match self.0 {
            ExecutionAdapter::Claude => {
                let worker_model = super::agent_worker_runtime::implementation_worker_model(
                    item,
                    workflow,
                    self.0,
                    &workflow.models.worker,
                    None,
                );
                super::agent_worker_runtime::spawn_claude_worker(
                    project_config,
                    item,
                    workflow,
                    pool,
                    &worker_model,
                )
                .await
            }
            ExecutionAdapter::Codex => {
                let agent_config = super::agent_worker_runtime::codex_agent_config_for_worker(
                    item, workflow, None,
                );
                super::codex_worker_spawn::spawn_worker(
                    project_config,
                    item,
                    workflow,
                    pool,
                    &agent_config,
                )
                .await
            }
            ExecutionAdapter::OpenCode => {
                super::opencode_worker_spawn::spawn_worker(project_config, item, workflow, pool)
                    .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), caller, task_id))]
    pub(crate) async fn start_structured(
        self,
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
        match self.0 {
            ExecutionAdapter::Codex => {
                let session = super::codex_structured::spawn_structured_session(
                    pool,
                    caller,
                    task_id,
                    project,
                    worker_name,
                    cwd,
                    prompt,
                    super::codex_output_schema::CodexOutputSchema(output_schema.0),
                    resume_thread_id,
                    agent_config,
                )
                .await?;
                Ok(AgentStructuredSession {
                    session_id: session.session_id,
                })
            }
            ExecutionAdapter::Claude | ExecutionAdapter::OpenCode => {
                anyhow::bail!(
                    "structured sessions are not enabled for {}",
                    self.0.as_str()
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), task_id = item.id, session_id))]
    pub(crate) async fn resume_worker(
        self,
        pool: &sqlx::SqlitePool,
        item: &Task,
        worker_name: &str,
        cwd: &std::path::Path,
        prompt: &str,
        session_id: &str,
        model: &str,
        workflow: &CaptainWorkflow,
        persisted_model: Option<&str>,
    ) -> Result<AgentWorkerResume> {
        match self.0 {
            ExecutionAdapter::Codex => {
                let agent_config = super::agent_worker_runtime::codex_agent_config_for_worker(
                    item,
                    workflow,
                    persisted_model,
                );
                let (pid, _stream_path, session_id) = super::codex_worker_control::resume_worker(
                    pool,
                    item,
                    worker_name,
                    cwd,
                    prompt,
                    session_id,
                    &agent_config,
                )
                .await?;
                Ok(AgentWorkerResume { pid, session_id })
            }
            ExecutionAdapter::Claude => {
                let worker_model = super::agent_worker_runtime::implementation_worker_model(
                    item,
                    workflow,
                    ExecutionAdapter::Claude,
                    model,
                    persisted_model,
                );
                let resume = super::claude_worker_control::resume_worker(
                    pool,
                    item,
                    worker_name,
                    cwd,
                    prompt,
                    session_id,
                    super::claude_worker_control::ClaudeRun {
                        model: &worker_model,
                        effort: workflow.agent.cc_effort,
                    },
                )
                .await?;
                Ok(AgentWorkerResume {
                    pid: resume.pid,
                    session_id: session_id.to_string(),
                })
            }
            ExecutionAdapter::OpenCode => {
                let resume = super::opencode_worker_spawn::resume_worker(
                    pool,
                    item,
                    worker_name,
                    cwd,
                    prompt,
                    session_id,
                    &workflow.stages.implementation,
                )
                .await?;
                Ok(AgentWorkerResume {
                    pid: resume.pid,
                    session_id: resume.session_id,
                })
            }
        }
    }

    pub(crate) fn resume_replacement_reason(
        self,
        stream_path: &std::path::Path,
        workflow: &CaptainWorkflow,
    ) -> Option<String> {
        match self.0 {
            ExecutionAdapter::Claude => {
                super::claude_worker_control::broken_resume_reason(stream_path, workflow)
            }
            ExecutionAdapter::Codex | ExecutionAdapter::OpenCode => None,
        }
    }

    pub(crate) fn uses_shared_process(self) -> bool {
        self.0 == ExecutionAdapter::Codex
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), task_id = item.id, worker = session_name))]
    pub(super) async fn start_rebase(
        self,
        item: &Task,
        pool: &sqlx::SqlitePool,
        session_name: &str,
        cwd: &std::path::Path,
        prompt: &str,
        model: &str,
        workflow: &CaptainWorkflow,
    ) -> Result<AdapterRebaseSession> {
        match self.task_owner()? {
            TaskOwnerProvider::Claude => {
                let session_id = global_infra::uuid::Uuid::v4().to_string();
                let credential = super::tick_spawn::pick_credential(pool).await;
                let credential_id = global_claude::credential_id(&credential);
                let mut env = std::collections::HashMap::new();
                if let Some((_id, token)) = &credential {
                    env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token.clone());
                }

                let (pid, _) = crate::io::process_manager::spawn_worker_process(
                    prompt,
                    cwd,
                    model,
                    workflow.agent.cc_effort,
                    &session_id,
                    &env,
                )
                .await?;

                if let Err(e) = crate::io::pid_registry::register(session_name, pid) {
                    tracing::warn!(module = "captain", worker = %session_name, %e, "pid_registry register failed");
                }
                if let Err(e) = crate::io::pid_registry::register(&session_id, pid) {
                    tracing::warn!(module = "captain", %session_id, %e, "pid_registry register (session_id) failed");
                }
                if let Err(e) = crate::io::headless_cc::log_running_session(
                    pool,
                    &session_id,
                    cwd,
                    model,
                    "rebase",
                    session_name,
                    Some(item.id),
                    false,
                    credential_id,
                )
                .await
                {
                    tracing::warn!(module = "captain", %session_id, error = %e, "failed to log rebase session");
                }

                Ok(AdapterRebaseSession { session_id, pid })
            }
            TaskOwnerProvider::Codex => {
                let started = super::codex_app_server::start_codex_turn(
                    cwd,
                    prompt,
                    None,
                    super::codex_app_server::CodexOutputMode::Text,
                    &workflow.agent,
                )
                .await?;
                let session = super::codex_session::begin_codex_session(
                    pool,
                    started,
                    super::codex_session::CodexSessionSpec {
                        caller: "rebase",
                        task_id: item.id,
                        project: &item.project,
                        worker_name: Some(session_name),
                        cwd,
                        prompt,
                        resumed: false,
                        alias: Some(session_name),
                        abort_reason: "rebase setup failed",
                    },
                )
                .await?;

                Ok(AdapterRebaseSession {
                    session_id: session.session_id,
                    pid: session.pid,
                })
            }
        }
    }

    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), session_id))]
    pub(crate) async fn steer(self, session_id: &str, message: String) -> Result<bool> {
        match self.0 {
            ExecutionAdapter::Codex => super::codex_app_server::steer(session_id, message).await,
            ExecutionAdapter::Claude | ExecutionAdapter::OpenCode => Ok(false),
        }
    }

    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), session_id))]
    pub(crate) async fn interrupt(self, session_id: &str) -> Result<()> {
        if self.0 == ExecutionAdapter::Codex {
            match super::codex_app_server::interrupt(session_id).await {
                Ok(true) => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
                Ok(false) => {}
                Err(e) => tracing::warn!(
                    module = "agent_runtime",
                    session_id,
                    error = %e,
                    "failed to interrupt agent turn before kill"
                ),
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(provider = %self.0.as_str(), session_id))]
    pub(crate) async fn terminate(self, session_id: &str) -> Result<()> {
        match self.0 {
            ExecutionAdapter::Codex => {
                self.interrupt(session_id).await?;
                if let Err(e) = crate::io::pid_registry::unregister(session_id) {
                    tracing::warn!(module = "agent_runtime", session_id, error = %e, "failed to unregister Codex session pid");
                }
                Ok(())
            }
            ExecutionAdapter::Claude => {
                super::claude_worker_control::terminate_worker_process(session_id).await
            }
            ExecutionAdapter::OpenCode => {
                super::opencode_worker_spawn::terminate_worker_process(session_id).await
            }
        }
    }
}
