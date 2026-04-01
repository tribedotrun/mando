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
    item_id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid item ID: {item_id}"))
}

fn parse_item_ids(ids: &[String]) -> Result<Vec<i64>> {
    ids.iter()
        .map(|s| s.parse())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| anyhow::anyhow!("invalid item IDs"))
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

/// Merge a PR for the given task, then mark item as merged.
pub(crate) async fn merge(bot: &TelegramBot, cid: &str, item_id: &str) -> Result<()> {
    let esc = mando_shared::escape_html(item_id);
    let id_num = parse_item_id(item_id)?;
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
            // Mark item as merged in the task list.
            if let Err(e) = gw.post(paths::TASKS_ACCEPT, &json!({"id": id_num})).await {
                error!("merge: PR merged but failed to update task for #{item_id}: {e}");
                bot.send_html(
                    cid,
                    &format!("\u{26a0}\u{fe0f} PR merged for #{esc} but task update failed: {e}"),
                )
                .await?;
                return Ok(());
            }
            info!("merge: PR merged and item #{item_id} marked merged");
            bot.send_html(cid, &format!("\u{1f389} Merged #{esc}"))
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

/// Request rework on a task.
pub(crate) async fn rework(bot: &TelegramBot, cid: &str, item_id: &str, title: &str) -> Result<()> {
    rework_with_feedback(
        bot,
        cid,
        item_id,
        title,
        "Restart cleanly and fix the outstanding issues.",
    )
    .await
}

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

/// Cancel all items from a multi-select picker.
pub(crate) async fn cancel_items(bot: &TelegramBot, cid: &str, ids: &[String]) -> Result<()> {
    let count = ids.len();
    let id_nums = parse_item_ids(ids)?;

    match bot
        .gw()
        .post(
            paths::TASKS_BULK,
            &json!({"ids": id_nums, "updates": {"status": "canceled"}}),
        )
        .await
    {
        Ok(_) => {
            info!("cancel: {count} item(s) cancelled");
            bot.send_html(cid, &format!("\u{274c} Cancelled {count} item(s)."))
                .await?;
        }
        Err(e) => {
            error!("cancel: bulk cancel failed: {e}");
            bot.send_html(cid, &format!("\u{274c} Cancel failed: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Delete (multi-select) ────────────────────────────────────────────

/// Delete all items from a multi-select picker.
pub(crate) async fn delete_items(bot: &TelegramBot, cid: &str, ids: &[String]) -> Result<()> {
    let count = ids.len();
    let id_nums = parse_item_ids(ids)?;

    match bot
        .gw()
        .post(paths::TASKS_DELETE, &json!({"ids": id_nums}))
        .await
    {
        Ok(_) => {
            info!("delete: {count} item(s) deleted");
            bot.send_html(cid, &format!("\u{1f5d1}\u{fe0f} Deleted {count} item(s)."))
                .await?;
        }
        Err(e) => {
            error!("delete: failed: {e}");
            bot.send_html(cid, &format!("\u{274c} Delete failed: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Todo confirm ─────────────────────────────────────────────────────

/// Write confirmed todo items to the task list via multipart POST.
pub(crate) async fn add_todo_items(
    bot: &TelegramBot,
    cid: &str,
    items: &[crate::bot::TodoItem],
) -> Result<()> {
    let mut ok_count = 0usize;

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
                ok_count += 1;
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

    if ok_count > 0 {
        bot.send_html(
            cid,
            &format!("\u{2705} Added {ok_count} item(s) to task list."),
        )
        .await?;
    }
    Ok(())
}

// ── Retry ────────────────────────────────────────────────────────────

/// Retry captain review for an errored task.
pub(crate) async fn retry_item(bot: &TelegramBot, cid: &str, item_id: &str) -> Result<()> {
    let esc = mando_shared::escape_html(item_id);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post(paths::TASKS_RETRY, &json!({"id": id_num}))
        .await
    {
        Ok(_) => {
            info!("retry: item #{item_id} retried");
            bot.send_html(cid, &format!("\u{1f504} Retry queued for #{esc}"))
                .await?;
        }
        Err(e) => {
            error!("retry: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Retry failed for #{esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Knowledge approve ────────────────────────────────────────────────

/// Approve a pending knowledge lesson.
pub(crate) async fn approve_knowledge(bot: &TelegramBot, cid: &str, id: &str) -> Result<()> {
    match bot
        .gw()
        .post(paths::KNOWLEDGE_APPROVE, &json!({"lessons": [{"id": id}]}))
        .await
    {
        Ok(_) => {
            info!("knowledge: approved lesson {id}");
            bot.send_html(cid, "\u{2705} Knowledge lesson approved.")
                .await?;
        }
        Err(e) => {
            error!("knowledge: approve failed for {id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Approve failed: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Cron kill ────────────────────────────────────────────────────────

/// Kill a worker process by PID (from cron_act callback).
pub(crate) async fn kill_worker(
    bot: &TelegramBot,
    cid: &str,
    mid: i64,
    pid_str: &str,
) -> Result<()> {
    let pid: u32 = pid_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid pid: {pid_str}"))?;

    match bot
        .gw()
        .post(&paths::worker_kill(pid_str), &json!({"pid": pid}))
        .await
    {
        Ok(_) => {
            info!("cron_act: killed worker pid={pid}");
            if let Err(e) = bot
                .edit_message(cid, mid, &format!("\u{1f480} Killed worker (pid {pid})."))
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        Err(e) => {
            error!("cron_act: kill failed for pid={pid}: {e}");
            if let Err(e) = bot
                .edit_message(cid, mid, &format!("\u{274c} Kill failed: {e}"))
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
    }
    Ok(())
}
