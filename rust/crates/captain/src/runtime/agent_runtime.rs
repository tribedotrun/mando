use anyhow::Result;
use settings::{CaptainWorkflow, Config, ProjectConfig};

use crate::Task;

pub(crate) use super::agent_liveness::{is_session_active, session_liveness, AgentLivenessStatus};
pub(crate) use super::agent_nudge::{nudge_worker, AgentNudgeOutcome};
pub(crate) use super::agent_session_result::{
    poll_structured_session_output, session_output_text, stream_meta_path, stream_path,
    AgentSessionOutput, AgentSessionPoll,
};
/// Provider-neutral worker spawn boundary.
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub async fn spawn_worker(
    config: &Config,
    project_slug: &str,
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<super::spawner::SpawnResult> {
    match item.provider {
        api_types::TaskProvider::Claude => {
            spawn_claude_worker(config, project_slug, project_config, item, workflow, pool).await
        }
        api_types::TaskProvider::Codex => {
            super::codex_worker_spawn::spawn_worker(project_config, item, workflow, pool).await
        }
    }
}

async fn spawn_claude_worker(
    config: &Config,
    project_slug: &str,
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
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

    let credential = super::tick_spawn::pick_credential(pool, Some("worker")).await;
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
        project_slug,
        project_config,
        &config.captain,
        workflow,
        pool,
        worker_cred.as_ref(),
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
    provider: global_types::TaskProvider,
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
    match provider {
        global_types::TaskProvider::Codex => {
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
        global_types::TaskProvider::Claude => {
            anyhow::bail!("structured AgentRuntime sessions are not enabled for Claude")
        }
    }
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
    match item.provider {
        global_types::TaskProvider::Codex => {
            let (pid, _stream_path, session_id) = super::codex_worker_control::resume_worker(
                pool,
                item,
                worker_name,
                cwd,
                prompt,
                session_id,
                &workflow.agent,
            )
            .await?;
            Ok(AgentWorkerResume { pid, session_id })
        }
        global_types::TaskProvider::Claude => {
            let resume = super::claude_worker_control::resume_worker(
                pool,
                item,
                worker_name,
                cwd,
                prompt,
                session_id,
                model,
            )
            .await?;
            Ok(AgentWorkerResume {
                pid: resume.pid,
                session_id: session_id.to_string(),
            })
        }
    }
}

pub(crate) fn worker_resume_replacement_reason(
    provider: global_types::TaskProvider,
    stream_path: &std::path::Path,
    workflow: &CaptainWorkflow,
) -> Option<String> {
    match provider {
        global_types::TaskProvider::Claude => {
            super::claude_worker_control::broken_resume_reason(stream_path, workflow)
        }
        global_types::TaskProvider::Codex => None,
    }
}

pub(crate) fn uses_shared_process(provider: global_types::TaskProvider) -> bool {
    matches!(provider, global_types::TaskProvider::Codex)
}

#[tracing::instrument(skip_all, fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn interrupt_session_before_kill(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> Result<()> {
    match provider {
        global_types::TaskProvider::Codex => match interrupt(provider, session_id).await {
            Ok(true) => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
            Ok(false) => {}
            Err(e) => tracing::warn!(
                module = "agent_runtime",
                session_id,
                error = %e,
                "failed to interrupt agent turn before kill"
            ),
        },
        global_types::TaskProvider::Claude => {}
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn terminate_worker_process(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> Result<()> {
    match provider {
        global_types::TaskProvider::Codex => {
            match super::codex_app_server::interrupt(session_id).await {
                Ok(true) => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
                Ok(false) => {}
                Err(e) => tracing::warn!(
                    module = "agent_runtime",
                    session_id,
                    error = %e,
                    "failed to interrupt Codex session before process termination"
                ),
            }
            if let Err(e) = crate::io::pid_registry::unregister(session_id) {
                tracing::warn!(module = "agent_runtime", session_id, error = %e, "failed to unregister Codex session pid");
            }
            Ok(())
        }
        global_types::TaskProvider::Claude => {
            super::claude_worker_control::terminate_worker_process(session_id).await
        }
    }
}

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
            )
            .await?;
            Ok(AgentRebaseSession {
                session_id: session.session_id,
                pid: session.pid,
            })
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
    }
}

#[tracing::instrument(fields(provider = %provider.as_str(), session_id))]
pub(crate) async fn interrupt(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> Result<bool> {
    match provider {
        global_types::TaskProvider::Codex => super::codex_app_server::interrupt(session_id).await,
        global_types::TaskProvider::Claude => Ok(false),
    }
}
