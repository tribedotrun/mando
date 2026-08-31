//! Task actions invoked by Telegram callbacks.
//!
//! Each function performs the mutation via HTTP calls to the gateway,
//! then reports success/failure back to the Telegram chat.

use anyhow::Result;
use tracing::{error, info};

use crate::bot::TelegramBot;
use crate::http::GatewayClient;

fn parse_item_id(item_id: &str) -> Result<i64> {
    global_types::parse_i64_id(item_id, "item").map_err(|e| anyhow::anyhow!(e))
}

/// Look up a task by ID via the gateway HTTP API.
///
/// Fail-fast: `Err` on infrastructure failure (invalid id, gateway
/// error). `Ok(None)` means the task genuinely doesn't exist. Collapsing
/// the two under `Option<_>`
/// previously made live tasks appear deleted whenever serde drift or a
/// transient gateway hiccup happened.
async fn find_task(gw: &GatewayClient, id: &str) -> anyhow::Result<Option<api_types::TaskItem>> {
    let id_num: i64 = id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid task id {id}: {e}"))?;
    let resp = gw
        .get_tasks(&api_types::TaskListQuery {
            include_archived: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("gateway failed to fetch task list: {e}"))?;
    Ok(resp.items.into_iter().find(|item| item.id == id_num))
}

// ── Merge ────────────────────────────────────────────────────────────

/// Initiate captain merge for a task's PR.
///
/// If `loading_mid` is `Some`, the result edits that message in-place.
/// If `None`, a loading placeholder is sent first and then edited.
pub(crate) async fn merge(
    bot: &TelegramBot,
    cid: &str,
    item_id: &str,
    loading_mid: Option<i64>,
) -> Result<()> {
    let esc = crate::telegram_format::escape_html(item_id);
    let gw = bot.gw();

    // Validate preconditions before sending loading message.
    let item = find_task(gw, item_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} not found"))?;
    let pr_num = item
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} has no PR"))?;
    let project = item
        .project
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("item #{item_id} has no project"))?;

    let mid = match loading_mid {
        Some(m) => m,
        None => bot.send_loading(cid, "\u{23f3} Merging\u{2026}").await?,
    };

    match gw
        .post_tasks_merge(&api_types::MergeRequest {
            pr_number: pr_num,
            project: project.clone(),
        })
        .await
    {
        Ok(_) => {
            info!("merge: captain merge initiated for #{item_id}");
            bot.edit_message(
                cid,
                mid,
                &format!("\u{1f680} Captain merge started for #{esc}"),
            )
            .await?;
        }
        Err(e) => {
            error!("merge: failed for #{item_id}: {e}");
            bot.edit_message(cid, mid, &format!("\u{274c} Merge failed for #{esc}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Accept ───────────────────────────────────────────────────────────

/// Accept (mark as merged) a task without triggering a PR merge.
///
/// If `loading_mid` is `Some`, the result edits that message in-place.
/// If `None`, a loading placeholder is sent first and then edited.
pub(crate) async fn accept(
    bot: &TelegramBot,
    cid: &str,
    item_id: &str,
    loading_mid: Option<i64>,
) -> Result<()> {
    let esc = crate::telegram_format::escape_html(item_id);
    let id_num = parse_item_id(item_id)?;
    let mid = match loading_mid {
        Some(m) => m,
        None => bot.send_loading(cid, "\u{23f3} Accepting\u{2026}").await?,
    };

    match bot
        .gw()
        .post_tasks_accept(&api_types::TaskIdRequest { id: id_num })
        .await
    {
        Ok(_) => {
            info!("accept: item #{item_id} accepted");
            bot.edit_message(cid, mid, &format!("\u{2705} Accepted #{esc}"))
                .await?;
        }
        Err(e) => {
            error!("accept: failed for #{item_id}: {e}");
            bot.edit_message(cid, mid, &format!("\u{274c} Accept failed for #{esc}: {e}"))
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
    let esc = crate::telegram_format::escape_html(title);
    let id_num = parse_item_id(item_id)?;
    let mid = bot
        .send_loading(cid, &format!("\u{23f3} Reopening: {esc}\u{2026}"))
        .await?;

    match bot
        .gw()
        .post_tasks_reopen(&api_types::TaskFeedbackRequest {
            id: id_num,
            feedback: feedback.to_string(),
        })
        .await
    {
        Ok(_) => {
            info!("reopen: item #{item_id} reopened with feedback");
            bot.edit_message(cid, mid, &format!("\u{1f504} Reopened: {esc}"))
                .await?;
        }
        Err(e) => {
            error!("reopen: failed for #{item_id}: {e}");
            bot.edit_message(cid, mid, &format!("\u{274c} Reopen failed for {esc}: {e}"))
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
    let esc = crate::telegram_format::escape_html(title);
    let id_num = parse_item_id(item_id)?;
    let mid = bot
        .send_loading(cid, &format!("\u{23f3} Reworking: {esc}\u{2026}"))
        .await?;

    match bot
        .gw()
        .post_tasks_rework(&api_types::TaskFeedbackRequest {
            id: id_num,
            feedback: feedback.to_string(),
        })
        .await
    {
        Ok(_) => {
            info!("rework: item #{item_id} sent to rework");
            bot.edit_message(cid, mid, &format!("\u{1f504} Rework: {esc}"))
                .await?;
        }
        Err(e) => {
            error!("rework: failed for #{item_id}: {e}");
            bot.edit_message(cid, mid, &format!("\u{274c} Rework failed for {esc}: {e}"))
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
    let esc = crate::telegram_format::escape_html(title);
    let id_num = parse_item_id(item_id)?;

    match bot
        .gw()
        .post_tasks_handoff(&api_types::TaskIdRequest { id: id_num })
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

// ── Stop (single task) ───────────────────────────────────────────────

/// Stop a single in-progress task. Kills the worker, transitions status to
/// `stopped`, preserves the worktree for reopen.
pub(crate) async fn stop(bot: &TelegramBot, cid: &str, item_id: &str) -> Result<()> {
    let id_num = parse_item_id(item_id)?;
    match bot
        .gw()
        .post_tasks_stop(&api_types::TaskIdRequest { id: id_num })
        .await
    {
        Ok(_) => {
            info!("stop: item #{item_id} stopped");
            bot.send_html(cid, &format!("\u{1f6d1} Stopped task #{item_id}"))
                .await?;
        }
        Err(e) => {
            error!("stop: failed for #{item_id}: {e}");
            bot.send_html(cid, &format!("\u{274c} Stop failed for #{item_id}: {e}"))
                .await?;
        }
    }
    Ok(())
}

// ── Todo add ─────────────────────────────────────────────────────────

/// Write one todo task via multipart POST.
///
/// If `loading_mid` is `Some`, the final summary edits that message in-place.
/// If `None`, the summary is sent as a new message.
pub(crate) async fn add_todo_item(
    bot: &TelegramBot,
    cid: &str,
    item: &crate::bot::TodoItem,
    loading_mid: Option<i64>,
) -> Result<()> {
    let mut fields = vec![("title", item.title.as_str()), ("source", "telegram")];
    if let Some(ref project) = item.project {
        fields.push(("project", project.as_str()));
    }

    let photo_data = if let Some(ref file_id) = item.photo_file_id {
        match bot.api().get_file(file_id).await {
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
    let added = match bot.gw().post_task_add_with_file(&fields, file_part).await {
        Ok(result) => {
            info!("todo: added '{}' -> {:?}", item.title, item.project);
            let id = result.id;
            let updates = api_types::TaskPatchRequest {
                context: None,
                original_prompt: Some(item.title.clone()),
                is_bug_fix: None,
            };
            if let Err(e) = bot
                .gw()
                .patch_tasks_by_id(&api_types::TaskIdParams { id }, &updates)
                .await
            {
                error!("todo: failed to set metadata for #{id}: {e}");
                if let Err(e) = bot
                    .send_html(
                        cid,
                        &format!("\u{26a0}\u{fe0f} Item #{id} added but metadata failed: {e}"),
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
            true
        }
        Err(e) => {
            error!("todo: failed to add '{}': {e}", item.title);
            if let Err(e) = bot
                .send_html(
                    cid,
                    &format!(
                        "\u{274c} Failed to add '{}': {e}",
                        crate::telegram_format::escape_html(&item.title),
                    ),
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            false
        }
    };

    if added {
        let project_label = item
            .project
            .as_deref()
            .map(|p| format!(" to <b>{}</b>", crate::telegram_format::escape_html(p)))
            .unwrap_or_default();
        let title = crate::telegram_format::escape_html(&item.title);
        let text = format!("\u{2705} Added 1 task(s){project_label}:\n\n1. {title}");
        if let Some(mid) = loading_mid {
            bot.edit_message(cid, mid, &text).await?;
        } else {
            bot.send_html(cid, &text).await?;
        }
    } else if let Some(mid) = loading_mid {
        bot.edit_message(cid, mid, "\u{274c} Task failed to add.")
            .await?;
    }
    Ok(())
}
