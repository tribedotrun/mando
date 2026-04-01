//! Research — AI-driven link discovery for a topic.

use std::time::Duration;

use anyhow::Result;
use mando_config::workflow::ScoutWorkflow;

/// Research result — a list of discovered links.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResearchResult {
    pub links: Vec<ResearchLink>,
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

/// Run research for a topic, returning discovered links.
pub async fn run_research(topic: &str, workflow: &ScoutWorkflow) -> Result<ResearchResult> {
    let interests_high = crate::biz::formatting::bullet_list(&workflow.interests.high);
    let interests_medium = crate::biz::formatting::bullet_list(&workflow.interests.medium);

    let user_context_rendered = workflow.user_context.render();

    let mut vars = std::collections::HashMap::new();
    vars.insert("topic", topic);
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("interests_medium", interests_medium.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = mando_config::render_prompt("research", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = workflow.models.get("research").cloned().unwrap_or_else(|| {
        tracing::warn!(
            module = "scout",
            "missing 'research' model in workflow config, using empty default"
        );
        String::new()
    });
    let result = mando_cc::CcOneShot::run(
        &prompt,
        mando_cc::CcConfig::builder()
            .model(model)
            .timeout(Duration::from_secs(300))
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
                            "required": ["url", "title"]
                        }
                    }
                },
                "required": ["links"]
            }))
            .build(),
    )
    .await?;

    if let Some(structured) = result.structured {
        let parsed: ResearchResult = serde_json::from_value(structured).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to parse LLM structured output, using empty result");
            ResearchResult::default()
        });
        return Ok(parsed);
    }
    let parsed: ResearchResult = mando_captain::biz::json_parse::parse_llm_json_as(&result.text)?;
    Ok(parsed)
}
