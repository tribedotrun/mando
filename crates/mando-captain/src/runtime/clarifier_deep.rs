//! Deep clarification — resumes an existing CC session to resolve remaining questions.

use std::collections::HashMap;
use std::time::Duration;

use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;
use tracing::warn;

use mando_cc::{CcConfig, CcOneShot};

use super::clarifier::{parse_clarifier_response, resolve_clarifier_cwd, ClarifierResult};

/// Run a deep clarification pass by resuming the existing clarifier session.
/// If the initial result already has `Ready` status or no questions, returns it unchanged.
pub(crate) async fn run_deep_clarification(
    item: &Task,
    workflow: &CaptainWorkflow,
    config: &mando_config::Config,
    pool: &sqlx::SqlitePool,
    initial: ClarifierResult,
) -> ClarifierResult {
    let questions = initial.questions.as_deref().unwrap_or("").trim();
    let Some(session_id) = initial.session_id.as_deref() else {
        return initial;
    };
    if questions.is_empty() {
        return initial;
    }

    let cwd = resolve_clarifier_cwd(item, config);
    let mut vars = HashMap::new();
    vars.insert("questions", questions);
    vars.insert("context", initial.context.as_str());

    let prompt = match mando_config::render_prompt("deep_clarifier", &workflow.prompts, &vars) {
        Ok(prompt) => prompt,
        Err(e) => {
            warn!(module = "clarifier", error = %e, "failed to render deep clarifier prompt");
            return ClarifierResult {
                deep_failed: true,
                ..initial
            };
        }
    };

    match CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model(&workflow.models.clarifier)
            .timeout(Duration::from_secs(workflow.agent.clarifier_timeout_s))
            .caller("deep-clarifier")
            .task_id(item.best_id())
            .cwd(cwd.clone())
            .resume(session_id.to_string())
            .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
            .json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["understood", "escalate"] },
                    "context": { "type": "string" },
                    "questions": { "type": ["string", "null"] }
                },
                "required": ["status", "context"]
            }))
            .build(),
    )
    .await
    {
        Ok(result) => {
            crate::io::headless_cc::log_cc_session(
                pool,
                &crate::io::headless_cc::SessionLogEntry {
                    session_id: &result.session_id,
                    cwd: &cwd,
                    model: &workflow.models.clarifier,
                    caller: "deep-clarifier",
                    cost_usd: result.cost_usd,
                    duration_ms: result.duration_ms,
                    resumed: true,
                    task_id: &item.best_id(),
                    status: mando_types::SessionStatus::Stopped,
                    worker_name: "",
                },
            )
            .await;
            let text = result
                .structured
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| result.text.clone());
            let mut parsed = parse_clarifier_response(&text, &item.title);
            parsed.session_id = Some(result.session_id);
            // Deep clarifier answers questions — it doesn't reassign project.
            // Carry forward the validated values from the initial clarification.
            parsed.repo = initial.repo;
            parsed.no_pr = initial.no_pr.or(parsed.no_pr);
            parsed.resource = initial.resource.or(parsed.resource);
            parsed
        }
        Err(e) => {
            warn!(module = "clarifier", error = %e, "deep clarifier failed — using shallow result");
            ClarifierResult {
                deep_failed: true,
                ..initial
            }
        }
    }
}
