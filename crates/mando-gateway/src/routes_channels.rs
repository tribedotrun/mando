//! /api/channels, /api/notify, /api/firecrawl/* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

/// GET /api/channels — show configured channels and their status.
pub(crate) async fn get_channels(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let tg_status = state.telegram_runtime.status().await;

    let tg = &config.channels.telegram;

    let mask_token = |t: &str| -> String {
        if t.len() > 4 {
            format!("{}***", &t[..4])
        } else if t.is_empty() {
            "(empty)".into()
        } else {
            "***".into()
        }
    };

    Json(json!({
        "channels": [
            {
                "name": "telegram",
                "enabled": tg_status.enabled,
                "running": tg_status.running,
                "mode": tg_status.mode,
                "token": mask_token(&tg.token),
                "owner": tg.owner,
                "lastError": tg_status.last_error,
            },
        ]
    }))
}

#[derive(Deserialize)]
pub(crate) struct NotifyBody {
    pub message: String,
    pub chat_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TelegramOwnerBody {
    pub owner: String,
}

pub(crate) async fn post_telegram_owner(
    State(state): State<AppState>,
    Json(body): Json<TelegramOwnerBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .config_manager
        .update(|cfg| {
            cfg.channels.telegram.owner = body.owner.clone();
            Ok(())
        })
        .await
        .map_err(|err| internal_error(err, "failed to update telegram config"))?;

    state
        .telegram_runtime
        .register_owner(body.owner)
        .await
        .map_err(|err| internal_error(err, "failed to register telegram owner"))?;

    Ok(Json(json!({"ok": true})))
}

pub(crate) async fn post_telegram_restart(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config_manager.load_full();
    if !config.channels.telegram.enabled || config.channels.telegram.token.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "telegram is disabled or missing a bot token",
        ));
    }

    state
        .telegram_runtime
        .restart()
        .await
        .map_err(|err| internal_error(err, "failed to restart telegram"))?;

    Ok(Json(json!({"ok": true})))
}

/// POST /api/notify — send a Telegram notification.
///
/// The daemon itself doesn't hold a Telegram connection (that's mando-tg),
/// so we emit a bus event that TG subscribers can pick up.
pub(crate) async fn post_notify(
    State(state): State<AppState>,
    Json(body): Json<NotifyBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let chat_id = body
        .chat_id
        .unwrap_or_else(|| config.channels.telegram.owner.clone());

    if chat_id.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "no chat_id provided and no owner configured",
        ));
    }

    // Emit a valid NotificationPayload for SSE subscribers.
    let payload = mando_types::events::NotificationPayload {
        message: body.message.clone(),
        level: mando_types::notify::NotifyLevel::Normal,
        kind: mando_types::events::NotificationKind::Generic,
        task_key: None,
        reply_markup: None,
    };
    state.bus.send(
        mando_types::BusEvent::Notification,
        Some(serde_json::to_value(&payload).unwrap_or(json!({"message": body.message}))),
    );

    tracing::info!(
        module = "notify",
        chat_id = %chat_id,
        "notification emitted"
    );

    Ok(Json(json!({"ok": true, "chat_id": chat_id})))
}

#[derive(Deserialize)]
pub(crate) struct FirecrawlScrapeBody {
    pub url: String,
}

/// POST /api/firecrawl/scrape — scrape a URL using Firecrawl API.
pub(crate) async fn post_firecrawl_scrape(
    Json(body): Json<FirecrawlScrapeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match mando_scout::runtime::firecrawl::scrape(&body.url).await {
        Ok(content) => Ok(Json(json!({"ok": true, "content": content}))),
        Err(e) => Err(internal_error(e, "firecrawl scrape failed")),
    }
}
