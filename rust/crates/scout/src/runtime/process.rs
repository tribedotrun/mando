//! Process orchestration — fetch content, summarize, persist to DB.
//!
//! Pipeline: fetch → extract → summarize → atomic DB update (title + scores
//! + summary + article + status).

use anyhow::{bail, Context, Result};
use rustc_hash::FxHashMap;
use settings::Config;
use settings::ScoutWorkflow;
use tracing::{info, warn};

use crate::io::content_fetch::{fetch_content, FetchedContent};
use crate::io::db::ScoutDb;
use crate::io::file_store;
use crate::runtime::article::{
    build_article_from_summary, build_local_article_if_short, normalize_article_markdown,
};
use crate::service::formatting::bullet_list;
use crate::service::url_detect::classify_url;
use crate::ScoutStatus;

/// Maximum retries for the summarize step.
const MAX_PROCESS_RETRIES: usize = 3;

/// Process a single item: fetch content, summarize, persist results.
#[tracing::instrument(skip_all)]
pub async fn process_item(
    _config: &Config,
    db: &ScoutDb,
    id: i64,
    workflow: &ScoutWorkflow,
) -> Result<()> {
    let item = db
        .get_item(id)
        .await?
        .ok_or(crate::ScoutError::NotFound(id))?;

    info!(id, url = %item.url, "process: starting");

    let content = fetch_claimed_content(db, id, &item).await?;

    if let Err(err) = process_item_with_content(db, id, workflow, &item, content).await {
        warn!(id, error = %err, "process: post-fetch failure, marking error");
        mark_error_if_status(db, id, &[ScoutStatus::Fetched], &err, "post-fetch failure").await;
        return Err(err);
    }

    Ok(())
}

async fn fetch_content_or_mark_error(
    db: &ScoutDb,
    id: i64,
    url: &str,
    error_statuses: &[ScoutStatus],
) -> Result<FetchedContent> {
    match fetch_content(url).await {
        Ok(content) => Ok(content),
        Err(err) => {
            warn!(id, error = %err, "process: fetch failed, marking error");
            mark_error_if_status(db, id, error_statuses, &err, "fetch failure").await;
            Err(err.context("content fetch failed"))
        }
    }
}

async fn mark_error_if_status(
    db: &ScoutDb,
    id: i64,
    allowed_statuses: &[ScoutStatus],
    original_error: &anyhow::Error,
    context: &'static str,
) {
    match db
        .increment_error_count_if_status(id, allowed_statuses)
        .await
    {
        Ok(true) => {}
        Ok(false) => warn!(
            id,
            context, "process: skipped error mark because item status changed"
        ),
        Err(mark_err) => {
            tracing::error!(
                module = "scout-runtime-process",
                id,
                error = %mark_err,
                original_error = %original_error,
                "process: failed to mark failure as error"
            );
        }
    }
}

async fn fetch_claimed_content(
    db: &ScoutDb,
    id: i64,
    item: &crate::ScoutItem,
) -> Result<FetchedContent> {
    match item.status {
        ScoutStatus::Pending | ScoutStatus::Error => {
            let content = fetch_content_or_mark_error(
                db,
                id,
                &item.url,
                &[ScoutStatus::Pending, ScoutStatus::Error],
            )
            .await?;
            let claimed = db
                .update_status_if(id, "fetched", &["pending", "error"])
                .await?;
            if !claimed {
                bail!("item #{id} is no longer pending/error; aborting stale scout processor");
            }
            info!(id, chars = content.text.len(), "process: content fetched");
            Ok(content)
        }
        ScoutStatus::Fetched => {
            // Retry path: cached body lives on disk. Metadata wasn't persisted
            // alongside the cache, so re-derive what we can from the URL —
            // tweet snowflakes work, everything else gets None and the caller
            // accepts that the deterministic extractors didn't fire.
            if let Some(text) = file_store::read_content_async(id).await {
                info!(
                    id,
                    chars = text.len(),
                    "process: retrying fetched item with cached content"
                );
                // Only tweet *status* URLs are valid snowflake inputs. Without
                // the host guard, any URL with `/status/` (GitHub, Notion,
                // Linear, etc.) would parse into a bogus 1970-era date.
                let extracted_date = if crate::io::content_fetch::is_tweet_status_url(&item.url) {
                    crate::io::metadata_probe::snowflake_date_from_tweet_url(&item.url)
                } else {
                    None
                };
                return Ok(FetchedContent {
                    text,
                    extracted_title: None,
                    extracted_date,
                });
            }
            warn!(
                id,
                "process: fetched item has no cached content, fetching again"
            );
            fetch_content_or_mark_error(db, id, &item.url, &[ScoutStatus::Fetched]).await
        }
        other => bail!("item #{id} is {other}; aborting non-processable scout item"),
    }
}

async fn process_item_with_content(
    db: &ScoutDb,
    id: i64,
    workflow: &ScoutWorkflow,
    item: &crate::ScoutItem,
    content: FetchedContent,
) -> Result<()> {
    let FetchedContent {
        text,
        extracted_title,
        extracted_date,
    } = content;

    // Write content to file immediately so the AI can read it via file path
    // (avoids inlining large transcripts into the prompt).
    if let Err(e) = file_store::write_content(id, &text) {
        warn!(id, %e, "process: content file write failed");
        return Err(e.into());
    }
    let content_path = file_store::content_path(id);
    let content_path_str = content_path.display().to_string();

    // Phase 2: AI scoring via headless Claude.
    // Title and date are deterministic now — extracted from yt-dlp info.json
    // (YouTube), snowflake math (tweets), or HTML meta probe / Firecrawl
    // metadata (everything else). The LLM scores and summarizes; it no
    // longer renames items or guesses dates from prose.
    let url_type = classify_url(&item.url);
    let final_title = extracted_title
        .clone()
        .or_else(|| item.title.clone())
        .unwrap_or_else(|| "Untitled".into());

    let source = crate::service::url_detect::derive_source_label(&item.url, url_type.as_str());

    // Build scoring prompt from workflow template.
    let interests_high = bullet_list(&workflow.interests.high);
    let interests_low = bullet_list(&workflow.interests.low);

    let user_context_rendered = workflow.user_context.render();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("url", item.url.as_str());
    vars.insert("title", final_title.as_str());
    vars.insert("url_type", url_type.as_str());
    vars.insert("content_path", content_path_str.as_str());
    vars.insert("interests_high", interests_high.as_str());
    vars.insert("interests_low", interests_low.as_str());
    vars.insert("user_context", user_context_rendered.as_str());

    let prompt = settings::render_prompt("process", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let model_owned = crate::service::model_lookup::required_model(workflow, "process")?;
    let model = model_owned.as_str();
    let timeout = workflow.agent.process_timeout_s;
    // Title and date_published were removed from the LLM schema on main —
    // they're now derived deterministically from yt-dlp / snowflake math /
    // HTML metadata before this prompt fires. Keep the slim schema while
    // routing through the failover wrapper from this branch.
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "source_name": { "type": "string" },
            "relevance_score": { "type": "integer" },
            "quality_score": { "type": "integer" },
            "summary": { "type": "string" }
        },
        "required": ["source_name", "relevance_score", "quality_score", "summary"]
    });
    let result = settings::cc_failover::run_with_credential_failover(
        db.pool(),
        "scout-process",
        &prompt,
        |ctx| {
            let mut builder = global_claude::CcConfig::builder()
                .model(model)
                .timeout(timeout)
                .caller("scout-process")
                .json_schema(schema.clone());
            builder = global_claude::with_credential(builder, &ctx.credential);
            if let Some(rid) = &ctx.resume_session_id {
                builder = builder.resume(rid);
            }
            builder.build()
        },
    )
    .await
    .with_context(|| format!("AI scoring call failed for #{id}"))?;
    let process_cred_id = result.credential_id;

    // Record session for this scout item.
    if let Err(e) = db
        .record_session(
            Some(id),
            &result.session_id,
            "scout-process",
            result.cost_usd,
            result.duration_ms,
            process_cred_id,
        )
        .await
    {
        warn!(id, error = %e, "process: failed to record session — cost tracking gap");
    }

    let parsed = match result.structured {
        Some(v) => v,
        None => global_claude::parse_llm_json(&result.text)
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
    let summary_text = parsed["summary"]
        .as_str()
        .with_context(|| {
            warn!(id, raw = %result.text, "process: missing summary");
            format!("AI response missing summary for #{id}")
        })?
        .to_string();

    // Prefer LLM-extracted source_name (e.g. YouTube channel) over URL-derived fallback.
    let source = parsed["source_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or(source);

    // Phase 3: Build summary markdown in memory.
    let date_line = extracted_date
        .as_deref()
        .map(|d| format!("**Published**: {d}\n"))
        .unwrap_or_default();
    let summary = format!(
        "# {final_title}\n\n\
         **Source**: {source}\n\
         **Type**: {}\n\
         {date_line}\
         **Relevance**: {relevance}/100 | **Quality**: {quality}/100\n\n\
         {summary_text}\n",
        url_type.as_str(),
    );

    // Phase 4: Article generation.
    let article_md = if let Some(local_article) =
        build_local_article_if_short(&final_title, &item.url, &text)
    {
        info!(
            id,
            chars = text.len(),
            "process: short source, using local article"
        );
        local_article
    } else {
        match crate::runtime::article::generate_article(
            &final_title,
            &item.url,
            url_type.as_str(),
            &content_path_str,
            workflow,
            db.pool(),
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
                        article_result.credential_id,
                    )
                    .await
                {
                    warn!(
                        id,
                        error = %e,
                        "process: failed to record article session — cost tracking gap"
                    );
                }
                normalize_article_markdown(&final_title, &article_result.text)
            }
            Err(e) => {
                tracing::error!(
                    module = "scout-runtime-process", id,
                    error = %e,
                    "process: article generation failed, falling back to summary article (degraded)"
                );
                build_article_from_summary(&final_title, &summary)
                    .unwrap_or_else(|| normalize_article_markdown(&final_title, &summary))
            }
        }
    };

    // Phase 5: Single atomic UPDATE — title + scores + summary + article +
    // processed status all commit together. After this point, "processed"
    // implies the readable payloads are guaranteed present in the row.
    let updated = db
        .update_processed(
            id,
            &final_title,
            relevance,
            quality,
            Some(&source),
            extracted_date.as_deref(),
            &summary,
            &article_md,
        )
        .await?;
    if !updated {
        info!(
            id,
            "process: final update lost status/rev guard, leaving item unchanged"
        );
        return Ok(());
    }

    // Telegraph publish is best-effort and runs after the DB commit so a
    // publish failure never blocks the item from appearing processed.
    match crate::io::telegraph::publish_article(id, &final_title, &article_md).await {
        Ok(url) => info!(id, %url, "process: published to Telegraph"),
        Err(e) => {
            warn!(id, %e, "process: Telegraph publish failed (non-fatal)")
        }
    }

    info!(id, title = %final_title, "process: complete");
    Ok(())
}

/// Process all pending items. Returns the number successfully processed.
#[tracing::instrument(skip_all)]
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
