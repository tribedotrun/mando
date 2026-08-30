use anyhow::Result;
use settings::{CaptainWorkflow, Config};

use crate::Task;

pub(crate) use super::agent_liveness::{is_session_active, session_liveness, AgentLivenessStatus};
pub(crate) use super::agent_nudge::{nudge_worker, AgentNudgeOutcome};
pub(crate) use super::agent_session_result::{
    poll_structured_session_output, session_output_text, stream_meta_path, stream_path,
    AgentSessionOutput, AgentSessionPoll,
};

pub(crate) use super::agent_worker_runtime::{
    claude_rebase_worker_model, implementation_provider, interrupt_session_before_kill,
    persisted_session_provider, persisted_worker_provider, resume_worker, spawn_structured_session,
    spawn_worker, terminate_worker_process, uses_shared_process, worker_resume_replacement_reason,
    AgentOutputSchema,
};

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub(crate) async fn spawn_clarifier_session(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<Option<String>> {
    match item.provider {
        global_types::TaskProvider::Codex => {
            super::codex_clarifier_dispatch::spawn_codex_clarifier(item, config, workflow, pool)
                .await
                .map(Some)
        }
        global_types::TaskProvider::Claude => Ok(None),
        global_types::TaskProvider::OpenCode => Ok(None),
    }
}

pub(crate) fn spawn_detached_clarifier_session(
    provider: global_types::TaskProvider,
    task: Task,
    workflow: CaptainWorkflow,
    config: Config,
    pool: sqlx::SqlitePool,
    session_id: String,
    task_tracker: &tokio_util::task::TaskTracker,
) {
    match provider {
        global_types::TaskProvider::Claude => super::claude_clarifier_dispatch::spawn_detached(
            task,
            workflow,
            config,
            pool,
            session_id,
            task_tracker,
        ),
        global_types::TaskProvider::Codex => tracing::warn!(
            module = "agent_runtime",
            task_id = task.id,
            "asked to spawn detached Claude-style clarifier for Codex task; ignoring"
        ),
        global_types::TaskProvider::OpenCode => tracing::warn!(
            module = "agent_runtime",
            task_id = task.id,
            "asked to spawn detached Claude-style clarifier for OpenCode task; ignoring"
        ),
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
    match item.provider {
        global_types::TaskProvider::Codex => {
            super::codex_review_spawn::spawn_codex_review(
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
        global_types::TaskProvider::Claude => {
            super::claude_review_spawn::spawn_claude_review(
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
        global_types::TaskProvider::OpenCode => {
            anyhow::bail!("review sessions are not enabled for OpenCode")
        }
    }
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
    match item.provider {
        global_types::TaskProvider::Codex => {
            super::codex_merge_spawn::spawn_codex_merge(
                item, cwd, notifier, pool, pr_url, pr_number, prompt, workflow,
            )
            .await
        }
        global_types::TaskProvider::Claude => {
            super::claude_merge_spawn::spawn_claude_merge(
                item, cwd, notifier, pool, pr_url, pr_number, prompt, workflow,
            )
            .await
        }
        global_types::TaskProvider::OpenCode => {
            anyhow::bail!("merge sessions are not enabled for OpenCode")
        }
    }
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
    match item.provider {
        global_types::TaskProvider::Codex => {
            let session = super::codex_rebase_spawn::spawn_rebase_worker(
                pool,
                item,
                session_name,
                cwd,
                prompt,
                &workflow.agent,
            )
            .await?;
            Ok(AgentRebaseSession {
                session_id: session.session_id,
                pid: session.pid,
            })
        }
        global_types::TaskProvider::Claude => {
            let session = super::claude_rebase_spawn::spawn_rebase_worker(
                item,
                pool,
                session_name,
                cwd,
                prompt,
                model,
                workflow.agent.cc_effort,
            )
            .await?;
            Ok(AgentRebaseSession {
                session_id: session.session_id,
                pid: session.pid,
            })
        }
        global_types::TaskProvider::OpenCode => {
            anyhow::bail!("rebase workers are not enabled for OpenCode")
        }
    }
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
    match item.provider {
        global_types::TaskProvider::Claude => {
            super::claude_clarifier_reclarify::answer_and_reclarify_claude(
                item,
                prompt,
                cwd,
                workflow,
                prior_resume_sid,
                pool,
            )
            .await
        }
        global_types::TaskProvider::Codex => {
            super::codex_clarifier_reclarify::answer_and_reclarify_codex(
                item,
                prompt,
                cwd,
                workflow,
                prior_resume_sid,
                pool,
            )
            .await
        }
        global_types::TaskProvider::OpenCode => {
            anyhow::bail!("clarifier sessions are not enabled for OpenCode")
        }
    }
}

#[tracing::instrument(skip(message), fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn steer(
    provider: global_types::TaskProvider,
    session_id: &str,
    message: String,
) -> Result<bool> {
    match provider {
        global_types::TaskProvider::Codex => {
            super::codex_app_server::steer(session_id, message).await
        }
        global_types::TaskProvider::Claude => Ok(false),
        global_types::TaskProvider::OpenCode => Ok(false),
    }
}
