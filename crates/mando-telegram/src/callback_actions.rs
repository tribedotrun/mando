//! Task actions invoked by Telegram callbacks.
//!
//! Each function performs the mutation via HTTP calls to the gateway,
//! then reports success/failure back to the Telegram chat.

use anyhow::Result;
use serde_json::json;
use tracing::{error, info};

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;
use crate::http::GatewayClient;

fn parse_item_id(item_id: &str) -> Result<i64> {
    mando_types::parse_i64_id(item_id, "item").map_err(|e| anyhow::anyhow!(e))
}

/// Look up a task by ID via the gateway HTTP API.
async fn find_task(gw: &GatewayClient, id: &str) -> Option<mando_types::Task> {
    let id_num: i64 = id.parse().ok()?;
    let resp = gw.get(paths::TASKS).await.ok()?;
    let items = resp.get("items")?.as_array()?;
    items.iter().find_map(|v| {
        if v.get("id")?.as_i64()? == id_num {
            serde_json::from_value(v.clone()).ok()
        } else {
            None
        }
    })
}

// ── Merge ────────────────────────────────────────────────────────────

/// Initiate captain merge for a task's PR.
pub(crate) async fn merge(bot: &TelegramBot, cid: &str, item_id: &str) -> Result<()> {
    let esc = mando_shared::escape_html(item_id);
    let gw = bot.gw();

    let item = find_task(gw, item_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} not found"))?;

    let pr = item
        .pr
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} has no PR"))?;
    let repo = item
        .project
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} has no project"))?;

    // Extract PR number from URL or bare number.
    let pr_number = pr.rsplit('/').next().unwrap_or(pr).trim_start_matches('#');

    match gw
        .post(
            paths::CAPTAIN_MERGE,
            &json!({"pr_num": pr_number, "project": repo}),
        )
        .await
    {
        Ok(_) => {
            info!("merge: captain merge initiated for #{item_id}");
            bot.send_html(cid, &format!("\u{1f680} Captain merge started for #{esc}"))
                .await?;
        }
        Err(e) => {
            error!("merge: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Merge failed for #{esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Accept ───────────────────────────────────────────────────────────

/// Accept (mark as merged) a task without triggering a PR merge.
pub(crate) async fn accept(bot: &TelegramBot, cid: &str, item_id: &str) -> Result<()> {
    let esc = mando_shared::escape_html(item_id);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post(paths::TASKS_ACCEPT, &json!({"id": id_num}))
        .await
    {
        Ok(_) => {
            info!("accept: item #{item_id} accepted");
            bot.send_html(cid, &format!("\u{2705} Accepted #{esc}"))
                .await?;
        }
        Err(e) => {
            error!("accept: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Accept failed for #{esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Reopen ───────────────────────────────────────────────────────────

/// Reopen a done/failed item with feedback from the user.
pub(crate) async fn reopen_with_feedback(
    bot: &TelegramBot,
    cid: &str,
    item_id: &str,
    title: &str,
    feedback: &str,
) -> Result<()> {
    let esc = mando_shared::escape_html(title);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post(
            paths::TASKS_REOPEN,
            &json!({"id": id_num, "feedback": feedback}),
        )
        .await
    {
        Ok(_) => {
            info!("reopen: item #{item_id} reopened with feedback");
            bot.send_html(cid, &format!("\u{1f504} Reopened: {esc}"))
                .await?;
        }
        Err(e) => {
            error!("reopen: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Reopen failed for {esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Rework ───────────────────────────────────────────────────────────

/// Request rework on a task with explicit operator feedback.
pub(crate) async fn rework_with_feedback(
    bot: &TelegramBot,
    cid: &str,
    item_id: &str,
    title: &str,
    feedback: &str,
) -> Result<()> {
    let esc = mando_shared::escape_html(title);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post(
            paths::TASKS_REWORK,
            &json!({"id": id_num, "feedback": feedback}),
        )
        .await
    {
        Ok(_) => {
            info!("rework: item #{item_id} sent to rework");
            bot.send_html(cid, &format!("\u{1f504} Rework: {esc}"))
                .await?;
        }
        Err(e) => {
            error!("rework: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Rework failed for {esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Handoff ──────────────────────────────────────────────────────────

/// Hand off an item to a human (kills worker if running, then sets status).
pub(crate) async fn handoff(
    bot: &TelegramBot,
    cid: &str,
    item_id: &str,
    title: &str,
) -> Result<()> {
    let esc = mando_shared::escape_html(title);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post(paths::TASKS_HANDOFF, &json!({"id": id_num}))
        .await
    {
        Ok(_) => {
            info!("handoff: item #{item_id} handed off");
            bot.send_html(cid, &format!("\u{1f91e} Handed off: {esc}"))
                .await?;
        }
        Err(e) => {
            error!("handoff: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Handoff failed for {esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Cancel (multi-select) ────────────────────────────────────────────

// ── Todo confirm ─────────────────────────────────────────────────────

/// Write confirmed todo items to the task list via multipart POST.
pub(crate) async fn add_todo_items(
    bot: &TelegramBot,
    cid: &str,
    items: &[crate::bot::TodoItem],
) -> Result<()> {
    let mut ok_titles: Vec<String> = Vec::new();

    for item in items {
        let mut fields = vec![("title", item.title.as_str())];
        if let Some(ref p) = item.project {
            fields.push(("project", p.as_str()));
        }

        // Download photo from Telegram if present
        let photo_data = if let Some(ref fid) = item.photo_file_id {
            match bot.api().get_file(fid).await {
                Ok(file_path) => match bot.api().download_file(&file_path).await {
                    Ok(bytes) => {
                        let ext = file_path.rsplit('.').next().unwrap_or("jpg");
                        Some((bytes, format!("photo.{ext}")))
                    }
                    Err(e) => {
                        error!("photo download failed: {e}");
                        if let Err(e) = bot
                            .send_html(cid, &format!("\u{26a0}\u{fe0f} Photo download failed: {e}"))
                            .await
                        {
                            tracing::warn!(module = "telegram", error = %e, "message send failed");
                        }
                        None
                    }
                },
                Err(e) => {
                    error!("getFile failed: {e}");
                    if let Err(e) = bot
                        .send_html(cid, &format!("\u{26a0}\u{fe0f} Photo fetch failed: {e}"))
                        .await
                    {
                        tracing::warn!(module = "telegram", error = %e, "message send failed");
                    }
                    None
                }
            }
        } else {
            None
        };

        let file_part = photo_data
            .as_ref()
            .map(|(bytes, name)| ("images", bytes.clone(), name.as_str()));
        match bot
            .gw()
            .post_multipart_with_file(paths::TASKS_ADD, &fields, file_part)
            .await
        {
            Ok(result) => {
                info!("todo: added '{}' -> {:?}", item.title, item.project);
                ok_titles.push(item.title.clone());
                let id_opt = result["id"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .or_else(|| result["id"].as_str().map(String::from));
                if let Some(id) = id_opt {
                    let updates = serde_json::json!({"original_prompt": item.title});
                    if let Err(e) = bot.gw().patch(&paths::task_item(&id), &updates).await {
                        error!("todo: failed to set metadata for #{id}: {e}");
                        if let Err(e) = bot
                            .send_html(
                                cid,
                                &format!(
                                    "\u{26a0}\u{fe0f} Item #{id} added but metadata failed: {e}"
                                ),
                            )
                            .await
                        {
                            tracing::warn!(module = "telegram", error = %e, "message send failed");
                        }
                    }
                } else {
                    error!("todo: POST succeeded but response missing 'id': {result}");
                }
            }
            Err(e) => {
                error!("todo: failed to add '{}': {e}", item.title);
                if let Err(e) = bot
                    .send_html(
                        cid,
                        &format!(
                            "\u{274c} Failed to add '{}': {e}",
                            mando_shared::escape_html(&item.title),
                        ),
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
        }
    }

    if !ok_titles.is_empty() {
        let list: String = ok_titles
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, mando_shared::escape_html(t)))
            .collect::<Vec<_>>()
            .join("\n");
        let project_label = items
            .iter()
            .find_map(|i| i.project.as_deref())
            .map(|p| format!(" to <b>{}</b>", mando_shared::escape_html(p)))
            .unwrap_or_default();
        bot.send_html(
            cid,
            &format!(
                "\u{2705} Added {} task(s){project_label}:\n\n{list}",
                ok_titles.len()
            ),
        )
        .await?;
    }
    Ok(())
}
