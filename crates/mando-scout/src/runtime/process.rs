//! Process orchestration — fetch content, summarize, update DB.
//!
//! Pipeline: fetch → extract → summarize → update DB → save summary file.

use anyhow::{bail, Context, Result};
use mando_config::workflow::ScoutWorkflow;
use mando_config::Config;
use rustc_hash::FxHashMap;
use tracing::{info, warn};

use crate::biz::formatting::{bullet_list, slugify_title};
use crate::biz::url_detect::classify_url;
use crate::io::content_fetch::fetch_content;
use crate::io::db::ScoutDb;
use crate::io::file_store;
use crate::runtime::article::{
    build_article_from_summary, build_local_article_if_short, normalize_article_markdown,
};

/// Maximum retries for the summarize step.
const MAX_PROCESS_RETRIES: usize = 3;

/// Process a single item: fetch content, summarize, persist results.
pub async fn process_item(
    _config: &Config,
    db: &ScoutDb,
    id: i64,
    workflow: &ScoutWorkflow,
) -> Result<()> {
    let item = db
        .get_item(id)
        .await?
        .with_context(|| format!("item #{id} not found"))?;

    info!(id, url = %item.url, "process: starting");

    // Phase 1: Fetch content
    let content = match fetch_content(&item.url).await {
        Ok(c) => {
            db.update_status_if(id, "fetched", &["pending", "error"])
                .await?;
            info!(id, chars = c.len(), "process: content fetched");
            c
        }
        Err(e) => {
            warn!(id, error = %e, "process: fetch failed, marking error");
            db.increment_error_count(id).await?;
            return Err(e.context("content fetch failed"));
        }
    };

    // Write content to file immediately so the AI can read it via file path
    // (avoids inlining large transcripts into the prompt).
    if let Err(e) = file_store::write_content(id, &content) {
        warn!(id, %e, "process: content file write failed, rolling back to pending");
        if let Err(re) = db.update_status(id, "pending").await {
            tracing::error!(id, rollback_error = %re, original_error = %e, "process: rollback to pending also failed — item may be stuck");
        }
        return Err(e.into());
    }
    let content_path = file_store::content_path(id);
    let content_path_str = content_path.display().to_string();

    // Phase 2: AI scoring via headless Claude
    let url_type = classify_url(&item.url);
    let title = item.title.clone().unwrap_or_else(|| "Untitled".into());

    let source = crate::biz::url_detect::derive_source_label(&item.url, url_type.as_str());

    // Build scoring prompt from workflow template.
    let interests_high = bullet_list(&workflow.interests.high);
    let interests_low = bullet_list(&workflow.interests.low);

    let user_context_rendered = workflow.user_context.render();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("url", item.url.as_str());
    vars.insert("title", title.as_str());
    vars.insert("url_type", url_type.as_str());
    vars.insert("content_path", content_path_str.as_str());
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("interests_low", interests_low.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = mando_config::render_prompt("process", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = crate::biz::model_lookup::required_model(workflow, "process")?;
    let result = mando_cc::CcOneShot::run(
        &prompt,
        mando_cc::CcConfig::builder()
            .model(model)
            .timeout(workflow.agent.process_timeout_s)
            .caller("scout-process")
            .json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "source_name": { "type": "string" },
                    "date_published": { "type": ["string", "null"] },
                    "relevance_score": { "type": "integer" },
                    "quality_score": { "type": "integer" },
                    "summary": { "type": "string" }
                },
                "required": ["title", "source_name", "relevance_score", "quality_score", "summary"]
            }))
            .build(),
    )
    .await
    .with_context(|| format!("AI scoring call failed for #{id}"))?;

    // Record session for this scout item.
    if let Err(e) = db
        .record_session(
            Some(id),
            &result.session_id,
            "scout-process",
            result.cost_usd,
            result.duration_ms,
        )
        .await
    {
        warn!(id, error = %e, "process: failed to record session — cost tracking gap");
    }

    let parsed = match result.structured {
        Some(v) => v,
        None => mando_captain::biz::json_parse::parse_llm_json(&result.text)
            .map_err(|e| anyhow::anyhow!("AI returned unparseable response for #{id}: {e}"))?,
    };

    let relevance = parsed["relevance_score"].as_i64().with_context(|| {
        warn!(id, raw = %result.text, "process: missing relevance_score");
        format!("AI response missing relevance_score for #{id}")
    })?;
    let quality = parsed["quality_score"].as_i64().with_context(|| {
        warn!(id, raw = %result.text, "process: missing quality_score");
        format!("AI response missing quality_score for #{id}")
    })?;
    let ai_title = parsed["title"]
        .as_str()
        .with_context(|| {
            warn!(id, raw = %result.text, "process: missing title");
            format!("AI response missing title for #{id}")
        })?
        .to_string();
    let summary_text = parsed["summary"]
        .as_str()
        .with_context(|| {
            warn!(id, raw = %result.text, "process: missing summary");
            format!("AI response missing summary for #{id}")
        })?
        .to_string();
    let date_published = parsed["date_published"].as_str().map(|s| s.to_string());

    // Prefer LLM-extracted source_name (e.g. YouTube channel) over URL-derived fallback.
    let source = parsed["source_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or(source);

    // Phase 3: Build files first while the item remains non-processed. This
    // keeps "processed" synonymous with "readable" across CLI/TG/Electron.
    let slug = slugify_title(&ai_title);

    let date_line = date_published
        .as_deref()
        .map(|d| format!("**Published**: {d}\n"))
        .unwrap_or_default();
    let summary = format!(
        "# {ai_title}\n\n\
         **Source**: {source}\n\
         **Type**: {}\n\
         {date_line}\
         **Relevance**: {relevance}/100 | **Quality**: {quality}/100\n\n\
         {summary_text}\n",
        url_type.as_str(),
    );
    file_store::write_summary(id, &slug, &summary)
        .with_context(|| format!("write summary for #{id}"))?;

    // Phase 4: Article generation + Telegraph publish for every processed item.
    let article_md = if let Some(local_article) =
        build_local_article_if_short(&ai_title, &item.url, &content)
    {
        info!(
            id,
            chars = content.len(),
            "process: short source, using local article"
        );
        local_article
    } else {
        match crate::runtime::article::generate_article(
            &ai_title,
            &item.url,
            url_type.as_str(),
            &content_path_str,
            workflow,
        )
        .await
        {
            Ok(article_result) => {
                if let Err(e) = db
                    .record_session(
                        Some(id),
                        &article_result.session_id,
                        "scout-article",
                        article_result.cost_usd,
                        article_result.duration_ms,
                    )
                    .await
                {
                    warn!(
                        id,
                        error = %e,
                        "process: failed to record article session — cost tracking gap"
                    );
                }
                normalize_article_markdown(&ai_title, &article_result.text)
            }
            Err(e) => {
                // TODO: add a `degraded: true` flag to the persisted article
                // so the UI can visibly mark this as a degraded rendering.
                // The Article struct doesn't expose such a field today;
                // for now we escalate the log level so ops can detect this.
                tracing::error!(
                    id,
                    error = %e,
                    "process: article generation failed, falling back to summary article (degraded)"
                );
                build_article_from_summary(&ai_title, &summary)
                    .unwrap_or_else(|| normalize_article_markdown(&ai_title, &summary))
            }
        }
    };
    file_store::write_article(id, &article_md)
        .with_context(|| format!("write article for #{id}"))?;
    match crate::io::telegraph::publish_article(id, &ai_title, &article_md).await {
        Ok(url) => info!(id, %url, "process: published to Telegraph"),
        Err(e) => {
            warn!(id, %e, "process: Telegraph publish failed (non-fatal)")
        }
    }

    // Phase 5: Mark the item processed only after the summary/article files
    // exist. If another worker raced us, clean up the staged files and fail.
    let updated = db
        .update_processed(
            id,
            &ai_title,
            relevance,
            quality,
            Some(&source),
            date_published.as_deref(),
        )
        .await?;
    if !updated {
        // Another concurrent run already processed this item — its files are
        // valid and must not be deleted.
        bail!("item #{id} already processed (status guard)");
    }
    if let Err(e) = file_store::delete_stale_summaries(id, &slug) {
        warn!(id, %e, "process: failed to delete stale summary files");
    }

    info!(id, title = %ai_title, "process: complete");
    Ok(())
}

/// Process all pending items. Returns the number successfully processed.
pub async fn process_all(config: &Config, db: &ScoutDb, workflow: &ScoutWorkflow) -> Result<usize> {
    let pending = db.list_processable().await?;
    if pending.is_empty() {
        info!("process_all: no processable items");
        return Ok(0);
    }

    info!(count = pending.len(), "process_all: starting batch");
    let mut success_count = 0;

    for item in &pending {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match process_item(config, db, item.id, workflow).await {
                Ok(()) => {
                    success_count += 1;
                    break;
                }
                Err(e) if attempt < MAX_PROCESS_RETRIES => {
                    warn!(
                        id = item.id,
                        attempt,
                        error = %e,
                        "process_all: retrying"
                    );
                }
                Err(e) => {
                    warn!(
                        id = item.id,
                        attempts = attempt,
                        error = %e,
                        "process_all: giving up"
                    );
                    break;
                }
            }
        }
    }

    info!(
        success = success_count,
        total = pending.len(),
        "process_all: complete"
    );
    Ok(success_count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn max_retries_constant() {
        assert_eq!(super::MAX_PROCESS_RETRIES, 3);
    }
}
