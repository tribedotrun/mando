//! Follow-up clarification turn: appends human answer and runs the
//! interactive clarifier inline.
//!
//! Extracted from `clarifier.rs` to keep that file under the 500-line
//! Rust cap and to group the multi-turn reclarification machinery
//! (prompt building + failover-aware dispatch + session logging) in one
//! place.

use anyhow::Result;
use global_claude::CcConfig;
use settings::CaptainWorkflow;
use tracing::{info, warn};

use crate::Task;

use super::clarifier::{
    build_interactive_clarifier_turn_prompt, parse_clarifier_response, resolve_clarifier_cwd,
    ClarifierQuestion, ClarifierResult,
};
use super::dashboard::truncate_utf8;

/// Unified clarification answer: appends human answer to context, runs
/// the interactive clarifier LLM inline, and returns the result. Resumes
/// an existing CC session when `task.session_ids.clarifier` is set.
#[tracing::instrument(skip_all)]
pub async fn answer_and_reclarify(
    item: &Task,
    answer: &str,
    workflow: &CaptainWorkflow,
    config: &settings::Config,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let questions = fetch_outstanding_questions(pool, item.id).await;
    let prompt =
        build_interactive_clarifier_turn_prompt(item, workflow, answer, questions.as_deref())?;
    let cwd = resolve_clarifier_cwd(item, config)?;
    let task_id = item.id.to_string();
    let timeout = workflow.agent.clarifier_timeout_s;

    let prior_resume_sid = resolve_prior_resume_sid(item, pool).await;

    let task_id_ref = task_id.as_str();
    let cwd_ref = cwd.as_path();
    let model = workflow.models.clarifier.as_str();
    let result = match settings::cc_failover::run_with_credential_failover(
        pool,
        "clarifier",
        &prompt,
        |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller("clarifier")
                .task_id(task_id_ref)
                .cwd(cwd_ref.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .json_schema(super::clarifier_cc_failure::build_interactive_clarifier_schema());
            builder = global_claude::with_credential(builder, &ctx.credential);
            // Failover wrapper's resume_session_id (the just-failed
            // session) takes precedence over the caller's pre-existing
            // clarifier session: after the first attempt hits a 429, the
            // transcript to continue from is what CC actually ran, not
            // what we entered with. On the first attempt they are the
            // same when prior_resume_sid is set.
            if let Some(rid) = ctx.resume_session_id.as_ref().or(prior_resume_sid.as_ref()) {
                builder = builder.resume(rid.clone());
            }
            builder.build()
        },
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            super::clarifier_cc_failure::log_reclarify_failure(pool, item, &cwd, &e).await;
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
            cwd: &cwd,
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

/// Pull the outstanding (unanswered) clarifier questions from the
/// timeline so the interactive turn prompt can reference them. A DB
/// error here is soft-failed — the turn still runs, just without prior
/// question context.
async fn fetch_outstanding_questions(
    pool: &sqlx::SqlitePool,
    task_id: i64,
) -> Option<Vec<ClarifierQuestion>> {
    match crate::io::queries::timeline::latest_clarifier_questions(pool, task_id).await {
        Ok(Some(payload_qs)) => Some(
            payload_qs
                .into_iter()
                .map(|q| ClarifierQuestion {
                    question: q.question,
                    answer: q.answer,
                    self_answered: q.self_answered,
                    category: q.category,
                })
                .collect(),
        ),
        Ok(None) => None,
        Err(e) => {
            warn!(
                module = "clarifier",
                task_id,
                error = %e,
                "failed to fetch outstanding questions from timeline"
            );
            None
        }
    }
}

/// Decide whether the failover wrapper's first attempt should
/// `--resume` the prior clarifier session. Belt-and-suspenders: reject
/// resume on a session id CC never issued. The deleted
/// `dispatch_reclarify` safety net used to allocate a fresh UUID into
/// `session_ids.clarifier` and this resume call errored with "no such
/// session". A missing `cc_sessions` row (`Ok(None)`) is NOT necessarily
/// a fake id — `log_cc_session` is best-effort, so a real CC session
/// can end up without a DB row if logging failed on the prior turn.
/// Fall back to running fresh in that case rather than hard-erroring
/// and stranding follow-up turns. The structural regression guard is
/// `check_no_random_session_ids.py`.
async fn resolve_prior_resume_sid(item: &Task, pool: &sqlx::SqlitePool) -> Option<String> {
    let sid = item.session_ids.clarifier.as_ref()?;
    match sessions_db::session_by_id(pool, sid).await {
        Ok(Some(_)) => Some(sid.clone()),
        Ok(None) => {
            warn!(
                module = "clarifier",
                %sid,
                "no cc_sessions row for prior clarifier session — running fresh (CC session logging may have failed earlier)"
            );
            None
        }
        Err(e) => {
            warn!(
                module = "clarifier",
                %sid,
                error = %e,
                "failed to verify prior clarifier session before resume — running fresh"
            );
            None
        }
    }
}
