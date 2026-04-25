//! Autonomous planning pipeline -- multi-agent iterative plan refinement.
//!
//! Pipeline: planner CC one-shot -> 3 feedback rounds (CC + Codex in parallel)
//! -> final synthesized plan with ASCII diagram.

use std::path::{Path, PathBuf};

use crate::{Task, TimelineEventPayload};
use anyhow::{Context, Result};
use global_claude::{codex_exec, CcConfig};
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;
use settings::Config;

pub struct PlanningResult {
    pub diagram: String,
    pub plan: String,
}

/// Run the full planning pipeline for a task.
///
/// Spawns multiple CC and Codex sessions, logs each for cost tracking,
/// and emits timeline events for live progress. Returns the final
/// diagram + plan text for the PlanCompleted event.
#[tracing::instrument(skip_all)]
pub(crate) async fn run_planning_pipeline(
    item: &Task,
    workflow: &CaptainWorkflow,
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> Result<PlanningResult> {
    let cwd = resolve_planning_cwd(item, config)?;
    let pcfg = &workflow.planning;

    // Step 1: Planner -- reads task context + codebase, produces draft plan.
    // Each sub-step picks its own credential via `cc_failover` so a single
    // exhausted credential mid-pipeline fails over to a healthy one
    // instead of aborting the whole plan.
    tracing::info!(
        module = "planning",
        task_id = item.id,
        "starting planner step"
    );
    let mut current_plan = run_planner_step(item, workflow, &cwd, pool).await?;

    // Step 2: Feedback rounds.
    let rounds = pcfg.feedback_rounds as usize;
    for round in 1..=rounds {
        tracing::info!(
            module = "planning",
            task_id = item.id,
            round,
            "starting feedback round"
        );
        let (cc_feedback, codex_feedback) =
            run_feedback_round(round, &current_plan, item, workflow, &cwd, pool).await?;

        // Emit progress timeline event.
        global_infra::best_effort!(
            super::timeline_emit::emit_for_task(
                item,
                &format!("Planning round {round}/{rounds} complete"),
                TimelineEventPayload::PlanningRound {
                    round: round as i64,
                    cc_feedback_len: cc_feedback.len() as i64,
                    codex_feedback_len: codex_feedback.len() as i64,
                },
                pool,
            )
            .await,
            "planning: super::timeline_emit::emit_for_task( item, &format!('Plannin"
        );
        // Step 2b: Synthesize feedback into next plan iteration.
        current_plan = run_synthesizer(
            round,
            &current_plan,
            &cc_feedback,
            &codex_feedback,
            item,
            workflow,
            &cwd,
            pool,
        )
        .await?;
    }

    // Step 3: Final synthesis -- produce diagram + concise plan.
    let result = run_final_synthesis(&current_plan, item, workflow, &cwd, pool).await?;

    Ok(result)
}

pub(crate) fn resolve_planning_cwd(item: &Task, config: &Config) -> Result<PathBuf> {
    match settings::resolve_project_config(Some(&item.project), config) {
        Some((_key, proj)) => Ok(global_infra::paths::expand_tilde(&proj.path)),
        None => Err(anyhow::anyhow!(
            "cannot resolve project {:?} for planning pipeline",
            item.project,
        )),
    }
}

async fn run_planner_step(
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let mut vars = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    let context = item.context.as_deref().unwrap_or("");
    vars.insert("context", context);
    vars.insert("project", item.project.as_str());

    let prompt = settings::render_prompt("planning_initial", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render planning_initial prompt: {e}"))?;

    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let model = workflow.models.captain.as_str();
    let timeout = workflow.planning.cc_timeout_s;
    let max_turns = workflow.planning.planner_max_turns;
    let result = settings::cc_failover::run_with_credential_failover(
        pool,
        "planning-planner",
        &prompt,
        |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller("planning-planner")
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .max_turns(max_turns);
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await?;

    log_planning_session(&result, cwd, "planning-planner", item.id, pool).await;
    Ok(result.text)
}

/// Run one feedback round: CC agent + Codex agent in parallel.
async fn run_feedback_round(
    round: usize,
    current_plan: &str,
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
    pool: &sqlx::SqlitePool,
) -> Result<(String, String)> {
    let cc_future = run_cc_feedback(round, current_plan, item, workflow, cwd, pool);
    let codex_future = run_codex_feedback(round, current_plan, item, workflow, cwd);

    let (cc_result, codex_result) = tokio::join!(cc_future, codex_future);

    let cc_feedback = cc_result.context("CC feedback agent failed")?;
    let codex_feedback = codex_result.unwrap_or_else(|e| {
        tracing::warn!(
            module = "planning",
            task_id = item.id,
            round,
            error = %e,
            "Codex feedback failed, using empty feedback"
        );
        String::from("(Codex feedback unavailable)")
    });

    Ok((cc_feedback, codex_feedback))
}

async fn run_cc_feedback(
    round: usize,
    current_plan: &str,
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let round_str = round.to_string();
    let mut vars = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    vars.insert("plan", current_plan);
    vars.insert("round", round_str.as_str());
    vars.insert("project", item.project.as_str());

    let prompt = settings::render_prompt("planning_cc_feedback", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render planning_cc_feedback prompt: {e}"))?;

    let caller = format!("planning-cc-r{round}");
    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let caller_ref = caller.as_str();
    let model = workflow.models.captain.as_str();
    let timeout = workflow.planning.cc_timeout_s;
    let max_turns = workflow.planning.feedback_max_turns;
    let result =
        settings::cc_failover::run_with_credential_failover(pool, caller_ref, &prompt, |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller(caller_ref)
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .max_turns(max_turns);
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        })
        .await?;

    log_planning_session(&result, cwd, &caller, item.id, pool).await;
    Ok(result.text)
}

async fn run_codex_feedback(
    round: usize,
    current_plan: &str,
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
) -> Result<String> {
    let round_str = round.to_string();
    let mut vars = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    vars.insert("plan", current_plan);
    vars.insert("round", round_str.as_str());
    vars.insert("project", item.project.as_str());

    let prompt = settings::render_prompt("planning_codex_feedback", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render planning_codex_feedback prompt: {e}"))?;

    let result = codex_exec(&prompt, cwd, workflow.planning.codex_timeout_s).await?;
    Ok(result.text)
}

#[allow(clippy::too_many_arguments)]
async fn run_synthesizer(
    round: usize,
    current_plan: &str,
    cc_feedback: &str,
    codex_feedback: &str,
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let round_str = round.to_string();
    let mut vars = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    vars.insert("plan", current_plan);
    vars.insert("cc_feedback", cc_feedback);
    vars.insert("codex_feedback", codex_feedback);
    vars.insert("round", round_str.as_str());

    let prompt = settings::render_prompt("planning_synthesize", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render planning_synthesize prompt: {e}"))?;

    let caller = format!("planning-synth-r{round}");
    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let caller_ref = caller.as_str();
    let model = workflow.models.captain.as_str();
    let timeout = workflow.planning.cc_timeout_s;
    let max_turns = workflow.planning.synthesizer_max_turns;
    let result =
        settings::cc_failover::run_with_credential_failover(pool, caller_ref, &prompt, |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller(caller_ref)
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .max_turns(max_turns);
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        })
        .await?;

    log_planning_session(&result, cwd, &caller, item.id, pool).await;
    Ok(result.text)
}

async fn run_final_synthesis(
    current_plan: &str,
    item: &Task,
    workflow: &CaptainWorkflow,
    cwd: &Path,
    pool: &sqlx::SqlitePool,
) -> Result<PlanningResult> {
    let mut vars = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    vars.insert("plan", current_plan);

    let prompt = settings::render_prompt("planning_final", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render planning_final prompt: {e}"))?;

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "diagram": {
                "type": "string",
                "description": "ASCII diagram summarizing the plan using box-drawing characters"
            },
            "plan": {
                "type": "string",
                "description": "Concise high-level plan text"
            }
        },
        "required": ["diagram", "plan"],
        "additionalProperties": false
    });

    let task_id = item.id.to_string();
    let task_id_ref = task_id.as_str();
    let model = workflow.models.captain.as_str();
    let timeout = workflow.planning.cc_timeout_s;
    let max_turns = workflow.planning.final_max_turns;
    let result = settings::cc_failover::run_with_credential_failover(
        pool,
        "planning-final",
        &prompt,
        |ctx| {
            let mut builder = CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller("planning-final")
                .task_id(task_id_ref)
                .cwd(cwd.to_path_buf())
                .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
                .json_schema(schema.clone())
                .max_turns(max_turns);
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await?;

    log_planning_session(&result, cwd, "planning-final", item.id, pool).await;

    let structured: serde_json::Value = result.structured.unwrap_or(serde_json::Value::Null);

    let diagram = structured["diagram"]
        .as_str()
        .unwrap_or("(diagram unavailable)")
        .to_string();
    let plan = structured["plan"]
        .as_str()
        .unwrap_or(&result.text)
        .to_string();

    Ok(PlanningResult { diagram, plan })
}

async fn log_planning_session(
    result: &global_claude::CcResult<serde_json::Value>,
    cwd: &Path,
    caller: &str,
    task_id: i64,
    pool: &sqlx::SqlitePool,
) {
    if let Err(e) =
        crate::io::headless_cc::log_cc_result(pool, result, cwd, caller, Some(task_id)).await
    {
        tracing::warn!(
            module = "planning",
            task_id,
            caller,
            error = %e,
            "failed to log planning session"
        );
    }
}
