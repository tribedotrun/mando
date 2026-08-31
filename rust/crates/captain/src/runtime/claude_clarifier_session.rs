//! Claude implementation for the provider-neutral clarifier phase.

use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures::FutureExt;
use global_claude::CcConfig;
use settings::{CaptainWorkflow, Config};
use sqlx::SqlitePool;
use tokio_util::task::TaskTracker;
use tracing::{info, warn};

use crate::Task;

use super::clarifier::{
    build_clarifier_prompt, build_clarifier_schema, parse_clarifier_response,
    resolve_clarifier_cwd, ClarifierResult,
};
use super::dashboard::truncate_utf8;

/// Run the clarification flow for a new Claude-owned task.
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id))]
async fn run_claude_initial(
    item: &Task,
    workflow: &CaptainWorkflow,
    config: &Config,
    pool: &sqlx::SqlitePool,
    pre_session_id: Option<&str>,
) -> Result<ClarifierResult> {
    let prompt = build_clarifier_prompt(item, None, workflow)?;
    let cwd = resolve_clarifier_cwd(item, config)?;
    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let cwd_ref = cwd.as_path();
    let model = workflow.models.clarifier.as_str();
    let timeout = workflow.agent.clarifier_timeout_s;
    let result = match settings::cc_failover::run_with_credential_failover(
        pool,
        "clarifier",
        &prompt,
        |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .effort(workflow.agent.cc_effort)
                .timeout(timeout)
                .caller("clarifier")
                .task_id(task_id_ref)
                .cwd(cwd_ref.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .json_schema(build_clarifier_schema(workflow).0);
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            } else if let Some(sid) = pre_session_id {
                builder = builder.session_id(sid);
            }
            builder.build()
        },
    )
    .await
    {
        Ok(result) => result,
        Err(e @ global_claude::CcError::Interrupted { .. }) => {
            info!(module = "clarifier", title = %item.title, "CC interrupted");
            return Err(e.into());
        }
        Err(e) => {
            warn!(module = "clarifier", title = %item.title, error = %e, "CC failed");
            return Err(e.into());
        }
    };

    if let Err(e) = crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd: &cwd,
            model: &workflow.models.clarifier,
            caller: "clarifier",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: false,
            task_id: Some(item.id),
            status: global_types::SessionStatus::Stopped,
            worker_name: "",
            credential_id: result.credential_id,
            error: None,
            api_error_status: None,
        },
    )
    .await
    {
        warn!(module = "clarifier", error = %e, "failed to log clarifier session");
    }

    let text = result
        .structured
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| result.text.clone());
    let mut parsed = parse_clarifier_response(&text, &item.title);
    parsed.session_id = Some(result.session_id.clone());
    info!(module = "clarifier", title = %truncate_utf8(&item.title, 60), status = ?parsed.status, "clarification complete");
    Ok(parsed)
}

pub(super) fn spawn_detached(
    task: Task,
    workflow: CaptainWorkflow,
    config: Config,
    pool: SqlitePool,
    session_id: String,
    task_tracker: &TaskTracker,
) {
    let session_id_for_panic = session_id.clone();
    let cwd = match resolve_clarifier_cwd(&task, &config) {
        Ok(cwd) => cwd,
        Err(e) => {
            tracing::error!(module = "captain", id = task.id, error = %e, "cannot resolve cwd for async clarifier — writing error");
            global_claude::write_error_result(
                &global_infra::paths::stream_path_for_session(&session_id),
                &format!("cannot resolve clarifier cwd: {e}"),
            );
            return;
        }
    };
    let cwd_for_failure = cwd.clone();
    let task_id_num = task.id;

    task_tracker.spawn(async move {
        let result = AssertUnwindSafe(async {
            match run_claude_initial(&task, &workflow, &config, &pool, Some(&session_id)).await {
                Ok(_) => tracing::info!(module = "captain", %session_id, "async clarifier completed"),
                Err(e) => {
                    if e.downcast_ref::<global_claude::CcError>()
                        .is_some_and(|error| matches!(error, global_claude::CcError::Interrupted { .. }))
                    {
                        tracing::info!(module = "captain", %session_id, "async clarifier interrupted");
                        return;
                    }
                    tracing::warn!(module = "captain", %session_id, error = %e, "async clarifier failed");
                    if let Some(global_claude::CcError::AllCredentialsExhausted {
                        earliest_reset,
                    }) = e.downcast_ref::<global_claude::CcError>()
                    {
                        if let Err(e2) = crate::io::queries::tasks::set_paused_until(
                            &pool,
                            task_id_num,
                            *earliest_reset,
                        )
                        .await
                        {
                            tracing::warn!(module = "captain", task_id = task_id_num, error = %e2, "failed to set paused_until on AllCredentialsExhausted");
                        } else {
                            tracing::warn!(module = "captain", task_id = task_id_num, earliest_reset, "task paused — all credentials rate-limited");
                        }
                    }
                    let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                    global_claude::write_error_result(
                        &stream_path,
                        &format!("clarifier failed: {e}"),
                    );
                    let error_text = format!("{e}");
                    let api_error_status = e
                        .downcast_ref::<global_claude::CcError>()
                        .and_then(global_claude::CcError::api_error_status);
                    if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                        &pool,
                        &session_id,
                        &cwd_for_failure,
                        "clarifier",
                        Some(task_id_num),
                        Some(&error_text),
                        api_error_status,
                    )
                    .await
                    {
                        tracing::warn!(module = "captain", %session_id, error = %e2, "log_cc_failure failed");
                    }
                }
            }
        })
        .catch_unwind()
        .await;

        if let Err(panic) = result {
            tracing::error!(module = "captain", session_id = %session_id_for_panic, "async clarifier panicked: {:?}", panic);
            global_claude::write_error_result(
                &global_infra::paths::stream_path_for_session(&session_id_for_panic),
                &format!("clarifier panicked: {:?}", panic),
            );
        }
    });
}
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id))]
pub(super) async fn answer_followup(
    item: &Task,
    prompt: &str,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    prior_resume_sid: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let model = workflow.models.clarifier.as_str();
    let timeout = workflow.agent.clarifier_timeout_s;
    let result = match settings::cc_failover::run_with_credential_failover(
        pool,
        "clarifier",
        prompt,
        |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .effort(workflow.agent.cc_effort)
                .timeout(timeout)
                .caller("clarifier")
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .json_schema(
                    super::clarifier_cc_failure::build_interactive_clarifier_schema(workflow).0,
                );
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = ctx.resume_session_id.as_deref().or(prior_resume_sid) {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            super::clarifier_cc_failure::log_reclarify_failure(pool, item, cwd, &e).await;
            return Err(e.into());
        }
    };

    if let Err(e) = crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd,
            model: &workflow.models.clarifier,
            caller: "clarifier",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: prior_resume_sid.is_some(),
            task_id: Some(item.id),
            status: global_types::SessionStatus::Stopped,
            worker_name: "",
            credential_id: result.credential_id,
            error: None,
            api_error_status: None,
        },
    )
    .await
    {
        warn!(module = "clarifier", error = %e, "failed to log clarifier session");
    }

    let text = result
        .structured
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| result.text.clone());
    let mut parsed = parse_clarifier_response(&text, &item.title);
    parsed.session_id = Some(result.session_id);
    info!(module = "clarifier", title = %truncate_utf8(&item.title, 60), status = ?parsed.status, "answer_and_reclarify complete");
    Ok(parsed)
}
