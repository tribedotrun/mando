//! Process orchestration — fetch content, summarize, update DB.
//!
//! Pipeline: fetch → extract → summarize → update DB → save summary file.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use mando_config::workflow::ScoutWorkflow;
use mando_config::Config;
use tracing::{info, warn};

use crate::biz::formatting::slugify_title;
use crate::biz::url_detect::classify_url;
use crate::io::content_fetch::fetch_content;
use crate::io::db::ScoutDb;
use crate::io::file_store;

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
    let interests_high = workflow
        .interests
        .high
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    let interests_medium = workflow
        .interests
        .medium
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    let interests_low = workflow
        .interests
        .low
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");

    let user_context_rendered = workflow.user_context.render();

    let mut vars = HashMap::new();
    vars.insert("url", item.url.as_str());
    vars.insert("title", title.as_str());
    vars.insert("url_type", url_type.as_str());
    vars.insert("content_path", content_path_str.as_str());
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("interests_medium", interests_medium.as_str());
    vars.insert("interests_low", interests_low.as_str());
    vars.insert("interests_tone", workflow.interests.tone.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = mando_config::render_prompt("process", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model = workflow.models.get("process").cloned().unwrap_or_else(|| {
        tracing::warn!(
            module = "scout",
            "missing 'process' model in workflow config, using empty default"
        );
        String::new()
    });
    let result = mando_cc::CcOneShot::run(
        &prompt,
        mando_cc::CcConfig::builder()
            .model(model)
            .timeout(std::time::Duration::from_secs(120))
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
            id,
            &result.session_id,
            "scout-process",
            result.cost_usd,
            result.duration_ms,
        )
        .await
    {
        warn!(id, error = %e, "process: failed to record session — cost tracking gap");
    }

    let parsed = result
        .structured
        .unwrap_or_else(|| mando_captain::biz::json_parse::parse_llm_json(&result.text));

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

    // Phase 3: Update DB with process results — fail fast if blocked
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
        bail!("item #{id} already processed (status guard)");
    }

    // Save files using the AI-generated title slug (matches what get_scout_item reads).
    // If file writes fail, roll back DB status so the item can be retried.
    let slug = slugify_title(&ai_title);

    let write_result = (|| -> Result<()> {
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

        Ok(())
    })();

    if let Err(e) = write_result {
        warn!(id, %e, "process: file write failed, restoring pre-process metadata and pending status");
        if let Err(rb_err) = db.rollback_processed(&item).await {
            tracing::error!(id, rollback_error = %rb_err, original_error = %e, "process: rollback_processed failed — row may have stale AI metadata with no summary file");
        }
        return Err(e);
    }

    // Phase 4: Article generation + Telegraph publish (YouTube only —
    // blogs/arxiv/repos are already readable; synthesis only adds value for
    // transcripts and audio-derived content).
    if url_type == crate::biz::url_detect::UrlType::YouTube {
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
                        id,
                        &article_result.session_id,
                        "scout-article",
                        article_result.cost_usd,
                        article_result.duration_ms,
                    )
                    .await
                {
                    warn!(id, error = %e, "process: failed to record article session — cost tracking gap");
                }
                let article_md = &article_result.text;
                if let Err(e) = file_store::write_article(id, article_md)
                    .with_context(|| format!("write article for #{id}"))
                {
                    warn!(id, %e, "process: article file write failed, skipping publish");
                } else {
                    match crate::io::telegraph::publish_article(id, &ai_title, article_md).await {
                        Ok(url) => info!(id, %url, "process: published to Telegraph"),
                        Err(e) => {
                            warn!(id, %e, "process: Telegraph publish failed (non-fatal)")
                        }
                    }
                }
            }
            Err(e) => warn!(id, %e, "process: article generation failed (non-fatal)"),
        }
    } else {
        info!(
            id,
            url_type = url_type.as_str(),
            "process: skipping article generation (not YouTube)"
        );
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
