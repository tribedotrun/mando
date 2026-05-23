//! Claude adapter for follow-up clarifier turns.

use anyhow::Result;
use global_claude::CcConfig;
use settings::CaptainWorkflow;
use tracing::{info, warn};

use crate::Task;

use super::clarifier::{parse_clarifier_response, ClarifierResult};
use super::dashboard::truncate_utf8;

#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id))]
pub(super) async fn answer_and_reclarify_claude(
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
                .timeout(timeout)
                .caller("clarifier")
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .json_schema(
                    super::clarifier_cc_failure::build_interactive_clarifier_schema(workflow).0,
                );
            builder = global_claude::with_credential(builder, &ctx.credential);
            // Failover wrapper's resume_session_id (the just-failed
            // session) takes precedence over the caller's pre-existing
            // clarifier session: after the first attempt hits a 429, the
            // transcript to continue from is what CC actually ran, not
            // what we entered with. On the first attempt they are the
            // same when prior_resume_sid is set.
            if let Some(rid) = ctx.resume_session_id.as_deref().or(prior_resume_sid) {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            super::clarifier_cc_failure::log_reclarify_failure(pool, item, cwd, &e).await;
            return Err(e.into());
        }
    };

    let cred_id = result.credential_id;
    // `resumed` reflects what CC saw on the first attempt — if the first
    // attempt resumed the prior session, the cc_sessions row records
    // `resumed=true` even if later failover attempts also resumed.
    let resumed = prior_resume_sid.is_some();
    if let Err(e) = crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd,
            model: &workflow.models.clarifier,
            caller: "clarifier",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed,
            task_id: Some(item.id),
            status: global_types::SessionStatus::Stopped,
            worker_name: "",
            credential_id: cred_id,
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

    info!(
        module = "clarifier",
        title = %truncate_utf8(&item.title, 60),
        status = ?parsed.status,
        "answer_and_reclarify complete"
    );
    Ok(parsed)
}
