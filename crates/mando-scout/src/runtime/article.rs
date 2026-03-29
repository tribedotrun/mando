//! Article generation — convert raw content into structured markdown articles.

use std::time::Duration;

use anyhow::Result;
use mando_config::workflow::ScoutWorkflow;

/// Result of article generation.
pub struct ArticleResult {
    pub text: String,
    pub session_id: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
}

/// Generate a full markdown article from raw content using the workflow's
/// `synthesize` prompt template.
pub async fn generate_article(
    title: &str,
    url: &str,
    url_type: &str,
    content_path: &str,
    workflow: &ScoutWorkflow,
) -> Result<ArticleResult> {
    let user_context_rendered = workflow.user_context.render();

    let mut vars = std::collections::HashMap::new();
    vars.insert("title", title);
    vars.insert("url", url);
    vars.insert("url_type", url_type);
    vars.insert("content_path", content_path);
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = mando_config::render_prompt("synthesize", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = workflow.models.get("article").cloned().unwrap_or_else(|| {
        tracing::warn!(
            module = "scout",
            "missing 'article' model in workflow config, using empty default"
        );
        String::new()
    });
    let result = mando_cc::CcOneShot::run(
        &prompt,
        mando_cc::CcConfig::builder()
            .model(model)
            .timeout(Duration::from_secs(300))
            .caller("scout-article")
            .build(),
    )
    .await?;

    Ok(ArticleResult {
        text: result.text,
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
    })
}
