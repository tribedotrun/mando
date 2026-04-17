//! Dashboard API handlers for scout operations.
//!
//! Each handler receives a pool, wraps it in ScoutDb, performs the operation, and returns JSON.

use crate::ScoutStatus;
use anyhow::{bail, Context, Result};
use rustc_hash::FxHashMap;
use serde_json::{json, Value};
use settings::config::Config;
use settings::config::ScoutWorkflow;
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::io::db::{ListQuery, ScoutDb};
use crate::io::file_store;
use crate::runtime::article::{
    build_article_from_summary, build_local_article_if_short, normalize_article_markdown,
};
use crate::service::url_detect::classify_url;

/// Valid user-settable statuses.
const USER_SETTABLE: &[&str] = &["pending", "processed", "saved", "archived"];

/// List scout items with optional search, status/type filter, and pagination.
pub async fn list_scout_items(
    pool: &SqlitePool,
    status: Option<&str>,
    search: Option<&str>,
    item_type: Option<&str>,
    page: Option<usize>,
    per_page: Option<usize>,
) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let q = ListQuery {
        search: search.filter(|s| !s.is_empty()).map(String::from),
        status: status.map(String::from),
        item_type: item_type.filter(|s| !s.is_empty()).map(String::from),
        page: page.unwrap_or(0),
        per_page: match per_page {
            Some(0) | None => 50,
            Some(n) => n,
        },
    };
    let result = db.query_items(&q).await?;
    let status_counts = db.count_by_status(&q).await?;
    let total_pages = result.total.div_ceil(q.per_page.max(1));

    let items_json: Vec<Value> = result
        .items
        .iter()
        .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
        .collect();

    Ok(json!({
        "items": items_json,
        "count": result.items.len(),
        "total": result.total,
        "page": q.page,
        "pages": total_pages,
        "per_page": q.per_page,
        "filter": status,
        "status_counts": status_counts,
    }))
}

/// Get a single scout item with its summary + article injected from the DB.
pub async fn get_scout_item(pool: &SqlitePool, id: i64) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let item = db
        .get_item(id)
        .await?
        .with_context(|| format!("item #{id} not found"))?;

    let summary = db.get_summary(id).await?;
    let article = db.get_article(id).await?;

    let mut val = serde_json::to_value(&item)?;
    if let Some(s) = summary {
        val["summary"] = Value::String(s);
    }
    if let (Some(title), Some(article_md)) = (item.title.as_deref(), article.as_deref()) {
        if let Some(tg_url) = crate::io::telegraph::get_cached_url_if_fresh(id, title, article_md) {
            val["telegraphUrl"] = Value::String(tg_url);
        }
    }
    Ok(val)
}

/// Get the full article content for a scout item.
pub async fn get_scout_article(pool: &SqlitePool, id: i64) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let item = db
        .get_item(id)
        .await?
        .with_context(|| format!("item #{id} not found"))?;

    let title = item.title.clone().unwrap_or_else(|| "Untitled".into());
    let article = db.get_article(id).await?;

    let mut val = json!({
        "id": id,
        "title": item.title,
        "article": article,
    });
    if let Some(article_md) = val["article"].as_str() {
        if let Some(tg_url) = crate::io::telegraph::get_cached_url_if_fresh(id, &title, article_md)
        {
            val["telegraphUrl"] = Value::String(tg_url);
        }
    }
    Ok(val)
}

/// Get the full article content for a scout item, healing stale processed
/// items on demand.
pub async fn ensure_scout_article(
    pool: &SqlitePool,
    id: i64,
    workflow: &ScoutWorkflow,
) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let item = db
        .get_item(id)
        .await?
        .with_context(|| format!("item #{id} not found"))?;

    let title = item.title.clone().unwrap_or_else(|| "Untitled".into());
    let mut article = db.get_article(id).await?;

    if article.is_none() && should_repair_article(item.status) {
        let content_path = file_store::content_path(id);
        let fallback_summary = db.get_summary(id).await?;
        if let Some(summary_article) =
            fallback_summary.and_then(|summary| build_article_from_summary(&title, &summary))
        {
            info!(id, title = %title, "scout: repairing article from current summary");
            db.set_article(id, &summary_article)
                .await
                .with_context(|| format!("write repaired article for #{id}"))?;
            crate::io::telegraph::invalidate_cache(id);
            article = Some(summary_article);
        } else if tokio::fs::try_exists(&content_path).await.unwrap_or(false) {
            info!(id, title = %title, "scout: healing stale or missing article");
            let raw_content = file_store::read_content_async(id).await.with_context(|| {
                format!("item #{id} content file exists but could not be read; refusing to heal with empty content")
            })?;
            let normalized = if let Some(local_article) =
                build_local_article_if_short(&title, &item.url, &raw_content)
            {
                info!(
                    id,
                    chars = raw_content.len(),
                    "scout: short source, using local article"
                );
                local_article
            } else {
                let article_result = crate::runtime::article::generate_article(
                    &title,
                    &item.url,
                    &item.item_type,
                    &content_path.display().to_string(),
                    workflow,
                    pool,
                )
                .await
                .with_context(|| format!("repair article for #{id}"))?;
                if let Err(e) = db
                    .record_session(
                        Some(id),
                        &article_result.session_id,
                        "scout-article-repair",
                        article_result.cost_usd,
                        article_result.duration_ms,
                        article_result.credential_id,
                    )
                    .await
                {
                    warn!(id, error = %e, "scout: failed to record article repair session");
                }
                normalize_article_markdown(&title, &article_result.text)
            };
            db.set_article(id, &normalized)
                .await
                .with_context(|| format!("write repaired article for #{id}"))?;
            crate::io::telegraph::invalidate_cache(id);
            article = Some(normalized);
        }
    }

    let mut val = json!({
        "id": id,
        "title": item.title,
        "article": article,
    });
    if let Some(article_md) = val["article"].as_str() {
        if let Some(tg_url) = crate::io::telegraph::get_cached_url_if_fresh(id, &title, article_md)
        {
            val["telegraphUrl"] = Value::String(tg_url);
        }
    }
    Ok(val)
}

fn should_repair_article(status: ScoutStatus) -> bool {
    matches!(
        status,
        ScoutStatus::Processed | ScoutStatus::Saved | ScoutStatus::Archived
    )
}

/// Add a URL to scout.
pub async fn add_scout_item(pool: &SqlitePool, url: &str, title: Option<&str>) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let url_type = classify_url(url);
    let (item, is_new) = db.add_item(url, url_type.as_str(), None).await?;

    if let Some(t) = title {
        if item.title.is_none() {
            db.set_title(item.id, t).await?;
        }
    }

    Ok(json!({
        "added": is_new,
        "id": item.id,
        "url": item.url,
        "type": item.item_type,
        "status": item.status.as_str(),
    }))
}

/// Delete a scout item, its cached files, and linked sessions.
pub async fn delete_scout_item(pool: &SqlitePool, id: i64) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());
    let existed = db.delete_item(id).await?;
    if !existed {
        anyhow::bail!("item #{id} not found");
    }
    file_store::delete_item_files(id);
    Ok(json!({
        "removed": true,
        "id": id,
    }))
}

/// Update item status.
pub async fn update_scout_status(pool: &SqlitePool, id: i64, status: &str) -> Result<()> {
    if !USER_SETTABLE.contains(&status) {
        anyhow::bail!(
            "invalid status '{status}', must be one of: {}",
            USER_SETTABLE.join(", ")
        );
    }
    let db = ScoutDb::new(pool.clone());
    db.update_status(id, status).await?;
    Ok(())
}

/// Process items — single item or all pending.
pub async fn process_scout(
    config: &Config,
    pool: &SqlitePool,
    id: Option<i64>,
    workflow: &ScoutWorkflow,
) -> Result<Value> {
    let db = ScoutDb::new(pool.clone());

    match id {
        Some(item_id) => {
            crate::runtime::process::process_item(config, &db, item_id, workflow).await?;
            Ok(json!({ "ok": true, "processed": 1 }))
        }
        None => {
            let count = crate::runtime::process::process_all(config, &db, workflow).await?;
            Ok(json!({ "ok": true, "processed": count }))
        }
    }
}

/// Generate a task from a scout item using AI.
pub async fn act_on_scout_item(
    config: &Config,
    pool: &SqlitePool,
    id: i64,
    project: &str,
    user_prompt: Option<&str>,
    workflow: &ScoutWorkflow,
) -> Result<Value> {
    let (_, project_config) = settings::config::resolve_project_config(Some(project), config)
        .with_context(|| format!("unknown project '{project}'"))?;
    let project_name = project_config.name.clone();
    let project_preamble = project_config.worker_preamble.clone();

    let db = ScoutDb::new(pool.clone());
    let item = db
        .get_item(id)
        .await?
        .with_context(|| format!("item #{id} not found"))?;

    let title = item.title.clone().unwrap_or_else(|| "Untitled".into());
    // Summary is optional context — if missing, the act prompt still works
    // using title + content.
    let summary = db.get_summary(id).await?.unwrap_or_default();
    let content = file_store::read_content(id)
        .filter(|c| !c.is_empty())
        .with_context(|| format!("item #{id} has no content — process it first"))?;
    let end = (0..=content.len().min(8000))
        .rev()
        .find(|&i| content.is_char_boundary(i))
        .unwrap_or(0);
    let truncated = &content[..end];

    let user_prompt_str = user_prompt.unwrap_or("");
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", title.as_str());
    vars.insert("url", item.url.as_str());
    vars.insert("summary", summary.as_str());
    vars.insert("content", truncated);
    vars.insert("project_name", project_name.as_str());
    vars.insert("project_preamble", project_preamble.as_str());
    vars.insert("user_prompt", user_prompt_str);

    let prompt = settings::config::render_prompt("act", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    info!(id, %project_name, "act: calling AI");

    let model = crate::service::model_lookup::required_model(workflow, "act")?;
    let credential = settings::io::credentials::pick_for_worker(pool, None)
        .await
        .inspect_err(|e| warn!(error = %e, "scout-act: pick_for_worker failed"))
        .unwrap_or(None);
    let cred_id = global_claude::credentials::credential_id(&credential);
    let builder = global_claude::CcConfig::builder()
        .model(model)
        .timeout(workflow.agent.act_timeout_s)
        .caller("scout-act")
        .json_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "description": { "type": "string" },
                "skip": { "type": "boolean" },
                "reason": { "type": "string" }
            },
            "required": ["skip"]
        }));
    let result = global_claude::CcOneShot::run(
        &prompt,
        global_claude::credentials::with_credential(builder, &credential).build(),
    )
    .await
    .with_context(|| format!("AI act call failed for #{id}"))?;

    if let Err(e) = db
        .record_session(
            Some(id),
            &result.session_id,
            "scout-act",
            result.cost_usd,
            result.duration_ms,
            cred_id,
        )
        .await
    {
        warn!(id, error = %e, "act: failed to record session — cost tracking gap");
    }

    let parsed = match result.structured {
        Some(v) => v,
        None => global_claude::json_parse::parse_llm_json(&result.text)
            .map_err(|e| anyhow::anyhow!("AI returned unparseable response for #{id}: {e}"))?,
    };
    if parsed.as_object().is_none_or(|o| o.is_empty()) {
        bail!("AI returned empty object for #{id}");
    }

    if parsed["skip"].as_bool() == Some(true) {
        let reason = parsed["reason"]
            .as_str()
            .unwrap_or("not actionable for this project")
            .to_string();
        info!(id, %reason, "act: skipped");
        return Ok(json!({ "skipped": true, "reason": reason }));
    }

    let task_title = parsed["title"]
        .as_str()
        .with_context(|| "AI response missing title")?
        .to_string();
    let task_description = parsed["description"]
        .as_str()
        .with_context(|| "AI response missing description")?
        .to_string();

    info!(id, %task_title, "act: creating task");

    Ok(json!({
        "task_title": task_title,
        "task_description": task_description,
        "project": project_name,
        "scout_item_id": id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_settable_statuses() {
        assert!(USER_SETTABLE.contains(&"pending"));
        assert!(USER_SETTABLE.contains(&"processed"));
        assert!(USER_SETTABLE.contains(&"saved"));
        assert!(USER_SETTABLE.contains(&"archived"));
        assert!(!USER_SETTABLE.contains(&"error"));
        assert!(!USER_SETTABLE.contains(&"fetched"));
    }
}
