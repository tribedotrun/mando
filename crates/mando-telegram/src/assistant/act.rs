//! Act flow handlers — project picker, prompt session, and API execution.

use anyhow::Result;

use mando_config::settings::Config;

use super::formatting::{act_project_picker_kb, act_prompt_kb};
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

/// Act button — show project picker or act directly if single project.
pub(super) async fn cb_act(
    bot: &mut TelegramBot,
    cb_id: &str,
    chat_id: &str,
    id: i64,
    config: &Config,
) -> Result<()> {
    let projects: Vec<String> = config
        .captain
        .projects
        .values()
        .map(|pc| pc.name.clone())
        .collect();

    if projects.is_empty() {
        bot.api
            .answer_callback_query(cb_id, Some("No projects configured"))
            .await?;
        return Ok(());
    }

    if projects.len() == 1 {
        return cb_act_with_project(bot, cb_id, chat_id, id, &projects[0]).await;
    }

    bot.api.answer_callback_query(cb_id, None).await?;
    let kb = act_project_picker_kb(id, &projects);
    bot.api
        .send_message(
            chat_id,
            &format!("\u{2699}\u{fe0f} Pick a project for scout item <b>#{id}</b>:"),
            Some("HTML"),
            Some(kb),
            true,
        )
        .await?;
    Ok(())
}

/// Act with a selected project — open prompt session for optional user context.
pub(super) async fn cb_act_with_project(
    bot: &mut TelegramBot,
    cb_id: &str,
    chat_id: &str,
    id: i64,
    project: &str,
) -> Result<()> {
    bot.api.answer_callback_query(cb_id, None).await?;
    bot.open_act_session(chat_id, id, project);

    let msg = format!(
        "\u{2699}\u{fe0f} Acting on <b>#{id}</b> in <b>{}</b>\n\n\
         Type additional context, or tap <b>Skip</b> to proceed without.",
        mando_shared::telegram_format::escape_html(project),
    );
    let kb = act_prompt_kb(id);
    bot.api
        .send_message(chat_id, &msg, Some("HTML"), Some(kb), true)
        .await?;
    Ok(())
}

/// Execute the act API call — shared by skip-button and text-prompt paths.
pub(crate) async fn execute_act(
    bot: &TelegramBot,
    chat_id: &str,
    id: i64,
    project: &str,
    prompt: Option<&str>,
) -> Result<()> {
    let working_msg = format!(
        "\u{2699}\u{fe0f} Creating task from scout item <b>#{id}</b> in <b>{}</b>\u{2026}",
        mando_shared::telegram_format::escape_html(project),
    );
    let sent = bot
        .api
        .send_message(chat_id, &working_msg, Some("HTML"), None, true)
        .await?;
    let sent_mid = sent["message_id"].as_i64().unwrap_or(0);

    let body = match prompt {
        Some(p) => serde_json::json!({"project": project, "prompt": p}),
        None => serde_json::json!({"project": project}),
    };
    let result_msg = match bot.gw().post(&paths::scout_act(id), &body).await {
        Ok(result) if result["skipped"].as_bool() == Some(true) => {
            let reason = result["reason"].as_str().unwrap_or("not actionable");
            format!(
                "\u{26a0}\u{fe0f} Skipped — {}",
                mando_shared::telegram_format::escape_html(reason),
            )
        }
        Ok(result) => {
            let task_id = result["task_id"].as_str().unwrap_or("?");
            let title = result["title"].as_str().unwrap_or("?");
            format!(
                "\u{2705} Task <b>#{task_id}</b> created in <b>{}</b>:\n{}",
                mando_shared::telegram_format::escape_html(project),
                mando_shared::telegram_format::escape_html(title),
            )
        }
        Err(e) => {
            format!(
                "\u{274c} Act failed: {}",
                mando_shared::telegram_format::escape_html(&e.to_string()),
            )
        }
    };

    if sent_mid != 0 {
        bot.api
            .edit_message_text(chat_id, sent_mid, &result_msg, Some("HTML"), None)
            .await?;
    } else {
        bot.api
            .send_message(chat_id, &result_msg, Some("HTML"), None, true)
            .await?;
    }
    Ok(())
}
