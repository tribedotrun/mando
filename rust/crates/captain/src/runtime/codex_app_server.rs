use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use settings::{AgentConfig, CodexSandboxPolicy};

pub(super) use super::codex_app_server_watch::watch_codex_turn;
use super::codex_output_schema::CodexOutputSchema;

pub(super) type CodexStarted = global_codex_app_server::StartedTurn;

pub(super) enum CodexOutputMode {
    Text,
    Structured(CodexOutputSchema),
}

#[tracing::instrument(skip(prompt, output_mode, agent_config), fields(cwd = %cwd.display(), resume = resume_thread_id.is_some()))]
pub(super) async fn start_codex_turn(
    cwd: &Path,
    prompt: &str,
    resume_thread_id: Option<&str>,
    output_mode: CodexOutputMode,
    agent_config: &AgentConfig,
) -> Result<CodexStarted> {
    let sandbox = codex_sandbox_params(cwd, agent_config.codex_sandbox_policy).await?;
    tracing::debug!(
        module = "codex_app_server",
        codex_sandbox_policy = %agent_config.codex_sandbox_policy.as_thread_start_str(),
        "starting Codex turn with sandbox policy"
    );
    let output_schema = match output_mode {
        CodexOutputMode::Text => None,
        CodexOutputMode::Structured(schema) => {
            Some(serde_json::to_value(schema.into_app_server_schema())?)
        }
    };
    global_codex_app_server::shared_manager()
        .start_turn(global_codex_app_server::StartTurnRequest {
            cwd: cwd.to_path_buf(),
            prompt: prompt.to_string(),
            resume_thread_id: resume_thread_id.map(str::to_string),
            output_schema,
            codex: codex_turn_config(agent_config),
            sandbox: sandbox.thread_sandbox.to_string(),
            sandbox_policy: sandbox.turn_policy,
            approval_policy: agent_config
                .codex_approval_policy
                .as_app_server_str()
                .to_string(),
            approvals_reviewer: agent_config
                .codex_approvals_reviewer
                .as_app_server_str()
                .to_string(),
            response_timeout: agent_config.ops_timeout_s,
        })
        .await
}

fn codex_turn_config(agent_config: &AgentConfig) -> global_codex_app_server::CodexTurnConfig {
    match &agent_config.codex {
        Some(config) => global_codex_app_server::CodexTurnConfig {
            model: config.model.clone(),
            reasoning_effort: config
                .reasoning_effort
                .map(|effort| effort.as_app_server_str().to_string()),
            service_tier: config.service_tier.clone(),
        },
        None => global_codex_app_server::CodexTurnConfig::default(),
    }
}

struct CodexSandboxParams {
    thread_sandbox: &'static str,
    turn_policy: Value,
}

async fn codex_sandbox_params(
    cwd: &Path,
    policy: CodexSandboxPolicy,
) -> Result<CodexSandboxParams> {
    match policy {
        CodexSandboxPolicy::WorkspaceWrite => workspace_write_params(cwd).await,
        CodexSandboxPolicy::DangerFullAccess => Ok(danger_full_access_params()),
    }
}

fn danger_full_access_params() -> CodexSandboxParams {
    CodexSandboxParams {
        thread_sandbox: CodexSandboxPolicy::DangerFullAccess.as_thread_start_str(),
        turn_policy: json!({ "type": CodexSandboxPolicy::DangerFullAccess.as_turn_policy_type() }),
    }
}

async fn workspace_write_params(cwd: &Path) -> Result<CodexSandboxParams> {
    let mut writable_roots = vec![
        cwd.display().to_string(),
        global_infra::paths::data_dir().display().to_string(),
    ];
    let common_git_dir = global_git::common_git_dir(cwd)
        .await
        .with_context(|| format!("resolve git common dir for {}", cwd.display()))?;
    writable_roots.push(common_git_dir.display().to_string());
    let git_dir = global_git::git_dir(cwd)
        .await
        .with_context(|| format!("resolve git dir for {}", cwd.display()))?;
    writable_roots.push(git_dir.display().to_string());
    writable_roots.sort();
    writable_roots.dedup();
    Ok(CodexSandboxParams {
        thread_sandbox: CodexSandboxPolicy::WorkspaceWrite.as_thread_start_str(),
        turn_policy: json!({
            "type": CodexSandboxPolicy::WorkspaceWrite.as_turn_policy_type(),
            "networkAccess": true,
            "writableRoots": writable_roots,
        }),
    })
}

#[tracing::instrument(skip(started), fields(thread_id = %started.thread_id))]
pub(super) async fn abort_started_turn(
    started: CodexStarted,
    extra_registry_key: Option<&str>,
    reason: &'static str,
) {
    let manager = global_codex_app_server::shared_manager();
    if let Err(e) = manager.interrupt(&started.thread_id).await {
        tracing::warn!(module = "codex_app_server", thread_id = %started.thread_id, error = %e, reason, "failed to interrupt Codex turn during abort");
    }
    if let Err(e) = manager
        .unsubscribe(&started.thread_id, started.response_timeout)
        .await
    {
        tracing::warn!(module = "codex_app_server", thread_id = %started.thread_id, error = %e, reason, "failed to unsubscribe Codex thread during abort");
    }
    manager.cleanup_thread(&started.thread_id).await;
    if let Err(e) = crate::io::pid_registry::unregister(&started.thread_id) {
        tracing::warn!(module = "codex_app_server", thread_id = %started.thread_id, error = %e, "failed to unregister Codex pid during abort");
    }
    if let Some(key) = extra_registry_key {
        if let Err(e) = crate::io::pid_registry::unregister(key) {
            tracing::warn!(module = "codex_app_server", registry_key = key, error = %e, "failed to unregister Codex alias pid during abort");
        }
    }
}

#[tracing::instrument(skip(message), fields(session_id))]
pub async fn steer(session_id: &str, message: String) -> Result<bool> {
    let manager = global_codex_app_server::shared_manager();
    let turn_id = manager.active_turn_id(session_id).await;
    let delivered = manager.steer(session_id, message.clone()).await?;
    if delivered {
        record_control_event(session_id, turn_id.as_deref(), "turn/steer", Some(&message)).await?;
    }
    Ok(delivered)
}

#[tracing::instrument(fields(session_id))]
pub async fn interrupt(session_id: &str) -> Result<bool> {
    let manager = global_codex_app_server::shared_manager();
    let turn_id = manager.active_turn_id(session_id).await;
    let delivered = manager.interrupt(session_id).await?;
    if delivered {
        record_control_event(session_id, turn_id.as_deref(), "turn/interrupt", None).await?;
    }
    Ok(delivered)
}

async fn record_control_event(
    session_id: &str,
    turn_id: Option<&str>,
    method: &'static str,
    message: Option<&str>,
) -> Result<()> {
    let path = global_infra::paths::codex_derived_stream_path_for_session(session_id);
    super::codex_stream::append_jsonl(
        &path,
        super::codex_stream::CodexStreamLine(json!({
            "type": "system",
            "subtype": "codex_control",
            "session_id": session_id,
            "turn_id": turn_id.unwrap_or(""),
            "method": method,
            "message": message,
        })),
    )
    .await?;
    if let Some(text) = message {
        super::codex_stream::append_jsonl(
            &path,
            super::codex_stream::CodexStreamLine(json!({
                "type": "user",
                "message": {"content": [{"type": "text", "text": text}]},
            })),
        )
        .await?;
    }
    Ok(())
}

#[tracing::instrument(fields(session_id))]
pub async fn is_turn_active(session_id: &str) -> bool {
    global_codex_app_server::shared_manager()
        .is_turn_active(session_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danger_full_access_params_match_codex_app_server_protocol() {
        let params = danger_full_access_params();

        assert_eq!(params.thread_sandbox, "danger-full-access");
        assert_eq!(params.turn_policy, json!({ "type": "dangerFullAccess" }));
    }

    #[test]
    fn workspace_write_policy_names_match_codex_app_server_protocol() {
        assert_eq!(
            CodexSandboxPolicy::WorkspaceWrite.as_thread_start_str(),
            "workspace-write"
        );
        assert_eq!(
            CodexSandboxPolicy::WorkspaceWrite.as_turn_policy_type(),
            "workspaceWrite"
        );
    }
}
