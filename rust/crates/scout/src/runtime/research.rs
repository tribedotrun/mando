//! Research — AI-driven link discovery for a topic.

use crate::{ScoutItem, ScoutResearchRun};
use anyhow::Result;
use rustc_hash::FxHashMap;
use settings::config::ScoutWorkflow;

/// Research result — a list of discovered links.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResearchResult {
    pub links: Vec<ResearchLink>,
}

/// Output from `run_research` — links plus session metadata for DB recording.
#[derive(Debug, Clone)]
pub struct ResearchOutput {
    pub result: ResearchResult,
    pub session_id: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub credential_id: Option<i64>,
}

/// A single discovered link.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ResearchLink {
    pub url: String,
    pub title: String,
    #[serde(rename = "type", alias = "link_type")]
    pub link_type: String,
    pub reason: String,
}

/// Run research for a topic, returning discovered links and session metadata.
#[tracing::instrument(skip_all)]
pub async fn run_research(
    topic: &str,
    workflow: &ScoutWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<ResearchOutput> {
    let interests_high = crate::service::formatting::bullet_list(&workflow.interests.high);

    let user_context_rendered = workflow.user_context.render();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("topic", topic);
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = settings::config::render_prompt("research", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = crate::service::model_lookup::required_model(workflow, "research")?;
    let credential = settings::credentials::pick_for_worker(pool, None)
        .await
        .inspect_err(|e| tracing::warn!(module = "scout-runtime-research", error = %e, "scout-research: pick_for_worker failed"))
        .unwrap_or(None);
    let cred_id = global_claude::credential_id(&credential);
    let builder = global_claude::CcConfig::builder()
        .model(model)
        .timeout(workflow.agent.research_timeout_s)
        .caller("scout-research")
        .json_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "links": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string" },
                            "title": { "type": "string" },
                            "type": { "type": "string" },
                            "reason": { "type": "string" }
                        },
                        "required": ["url", "title", "type", "reason"]
                    }
                }
            },
            "required": ["links"]
        }));
    let result = global_claude::CcOneShot::run_with_retry(
        &prompt,
        global_claude::with_credential(builder, &credential).build(),
        workflow.agent.cc_max_retries,
    )
    .await?;

    let parsed: ResearchResult = if let Some(structured) = result.structured {
        serde_json::from_value(structured).map_err(|e| {
            anyhow::anyhow!("failed to parse LLM structured output for research: {e}")
        })?
    } else {
        global_claude::parse_llm_json_as(&result.text)?
    };

    Ok(ResearchOutput {
        result: parsed,
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        credential_id: cred_id,
    })
}

#[tracing::instrument(skip_all)]
pub async fn insert_run(pool: &sqlx::SqlitePool, topic: &str) -> Result<i64> {
    crate::io::queries::scout_research::insert_run(pool, topic).await
}

#[tracing::instrument(skip_all)]
pub async fn complete_run(
    pool: &sqlx::SqlitePool,
    id: i64,
    session_id: &str,
    added_count: i64,
) -> Result<()> {
    crate::io::queries::scout_research::complete_run(pool, id, session_id, added_count).await
}

#[tracing::instrument(skip_all)]
pub async fn fail_run(pool: &sqlx::SqlitePool, id: i64, error: &str) -> Result<()> {
    crate::io::queries::scout_research::fail_run(pool, id, error).await
}

#[tracing::instrument(skip_all)]
pub async fn list_runs(pool: &sqlx::SqlitePool, limit: i64) -> Result<Vec<ScoutResearchRun>> {
    crate::io::queries::scout_research::list_runs(pool, limit).await
}

#[tracing::instrument(skip_all)]
pub async fn get_run(pool: &sqlx::SqlitePool, id: i64) -> Result<Option<ScoutResearchRun>> {
    crate::io::queries::scout_research::get_run(pool, id).await
}

#[tracing::instrument(skip_all)]
pub async fn list_items_by_run(pool: &sqlx::SqlitePool, run_id: i64) -> Result<Vec<ScoutItem>> {
    crate::io::queries::scout::list_items_by_run(pool, run_id).await
}

#[tracing::instrument(skip_all)]
pub async fn set_research_run_id(pool: &sqlx::SqlitePool, id: i64, run_id: i64) -> Result<()> {
    crate::io::queries::scout::set_research_run_id(pool, id, run_id).await
}

#[tracing::instrument(skip_all)]
pub async fn reset_error_state(pool: &sqlx::SqlitePool, id: i64) -> Result<()> {
    crate::io::queries::scout::reset_error_state(pool, id).await
}
