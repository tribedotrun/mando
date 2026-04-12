//! Research — AI-driven link discovery for a topic.

use anyhow::Result;
use mando_config::workflow::ScoutWorkflow;
use rustc_hash::FxHashMap;

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
}

/// A single discovered link.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResearchLink {
    pub url: String,
    pub title: String,
    #[serde(default, rename = "type", alias = "link_type")]
    pub link_type: String,
    #[serde(default)]
    pub reason: String,
}

/// Run research for a topic, returning discovered links and session metadata.
pub async fn run_research(topic: &str, workflow: &ScoutWorkflow) -> Result<ResearchOutput> {
    let interests_high = crate::biz::formatting::bullet_list(&workflow.interests.high);

    let user_context_rendered = workflow.user_context.render();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("topic", topic);
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = mando_config::render_prompt("research", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = crate::biz::model_lookup::required_model(workflow, "research")?;
    let result = mando_cc::CcOneShot::run(
        &prompt,
        mando_cc::CcConfig::builder()
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
            }))
            .build(),
    )
    .await?;

    let parsed: ResearchResult = if let Some(structured) = result.structured {
        serde_json::from_value(structured).map_err(|e| {
            anyhow::anyhow!("failed to parse LLM structured output for research: {e}")
        })?
    } else {
        mando_captain::biz::json_parse::parse_llm_json_as(&result.text)?
    };

    Ok(ResearchOutput {
        result: parsed,
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
    })
}
