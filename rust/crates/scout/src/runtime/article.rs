//! Article generation — convert raw content into structured markdown articles.

use anyhow::Result;
use rustc_hash::FxHashMap;
use settings::ScoutWorkflow;

const LOCAL_ARTICLE_MAX_CHARS: usize = 280;

/// Result of article generation.
pub struct ArticleResult {
    pub text: String,
    pub session_id: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub credential_id: Option<i64>,
}

/// Generate a full markdown article from raw content using the workflow's
/// `synthesize` prompt template.
#[tracing::instrument(skip_all)]
pub async fn generate_article(
    title: &str,
    url: &str,
    url_type: &str,
    content_path: &str,
    workflow: &ScoutWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<ArticleResult> {
    let user_context_rendered = workflow.user_context.render();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", title);
    vars.insert("url", url);
    vars.insert("url_type", url_type);
    vars.insert("content_path", content_path);
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = settings::render_prompt("synthesize", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model_owned = crate::service::model_lookup::required_model(workflow, "article")?;
    let model = model_owned.as_str();
    let timeout = workflow.agent.article_timeout_s;
    let result = settings::cc_failover::run_with_credential_failover(
        pool,
        "scout-article",
        &prompt,
        |ctx| {
            let mut builder = global_claude::CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller("scout-article");
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await?;
    let cred_id = result.credential_id;

    Ok(ArticleResult {
        text: result.text,
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        credential_id: cred_id,
    })
}

/// Normalize generated markdown into a stable article shape.
///
/// Fresh generations should always start with the exact current title as H1 and
/// should never keep chatty preambles before that title.
pub fn normalize_article_markdown(title: &str, article_md: &str) -> String {
    let normalized_title = format!("# {title}");
    let trimmed = article_md.trim();

    if trimmed.is_empty() {
        return normalized_title;
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    if let Some(idx) = lines.iter().position(|line| line.trim().starts_with("# ")) {
        let tail = lines
            .iter()
            .skip(idx + 1)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if tail.is_empty() {
            return normalized_title;
        }
        return format!("{normalized_title}\n\n{tail}");
    }

    format!("{normalized_title}\n\n{trimmed}")
}

/// Reuse a current summary as an immediately readable article when the cached
/// article is stale or missing.
pub fn build_article_from_summary(title: &str, summary_md: &str) -> Option<String> {
    let trimmed = summary_md.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(normalize_article_markdown(title, trimmed))
}

/// For very short sources, skip expensive synthesis and render a direct article.
pub fn build_local_article_if_short(title: &str, url: &str, raw_content: &str) -> Option<String> {
    let cleaned = raw_content.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() || cleaned.chars().count() > LOCAL_ARTICLE_MAX_CHARS {
        return None;
    }

    Some(format!(
        "# {title}\n\n\
         ## Source\n\
         - URL: {url}\n\n\
         ## What It Says\n\
         {cleaned}\n\n\
         ## Why This Matters\n\
         This source is short enough that the extracted text above is the full signal. \
         A longer synthesized article would mostly add invented filler.\n\n\
         ## Key Takeaways\n\
         - The source is brief and self-contained.\n\
         - There is not enough material for a longer synthesized article without fabrication.\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        build_article_from_summary, build_local_article_if_short, normalize_article_markdown,
    };

    #[test]
    fn normalize_article_strips_chatty_preamble_and_rewrites_heading() {
        let raw = "Sylvie's Dad, here's the formatted article:\n\n---\n\n# Wrong Title\n\nBody";
        let normalized = normalize_article_markdown("Correct Title", raw);
        assert_eq!(normalized, "# Correct Title\n\nBody");
    }

    #[test]
    fn normalize_article_prepends_title_when_missing() {
        let raw = "Intro paragraph.\n\n## Section\n\nBody";
        let normalized = normalize_article_markdown("Correct Title", raw);
        assert!(normalized.starts_with("# Correct Title\n\nIntro paragraph."));
    }

    #[test]
    fn short_sources_use_local_article_builder() {
        let article = build_local_article_if_short(
            "Example",
            "https://example.com",
            "Example Domain This domain is for use in documentation examples.",
        )
        .expect("short source should use local article builder");
        assert!(article.contains("# Example"));
        assert!(article.contains("## What It Says"));
        assert!(article.contains("documentation examples"));
    }

    #[test]
    fn summary_fallback_rewrites_heading() {
        let article =
            build_article_from_summary("Correct Title", "# Old Title\n\n- Bullet 1\n- Bullet 2")
                .expect("summary should build fallback article");
        assert!(article.starts_with("# Correct Title"));
        assert!(article.contains("- Bullet 1"));
    }
}
