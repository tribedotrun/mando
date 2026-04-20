//! /api/channels, /api/notify, /api/firecrawl/* route handlers.

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

/// GET /api/channels — show configured channels and their status.
#[crate::instrument_api(method = "GET", path = "/api/channels")]
pub(crate) async fn get_channels(
    State(state): State<AppState>,
) -> Json<api_types::ChannelsResponse> {
    let config = state.settings.load_config();
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

    Json(api_types::ChannelsResponse {
        channels: vec![api_types::ChannelStatus {
            name: "telegram".to_string(),
            enabled: tg_status.enabled,
            running: tg_status.running,
            mode: tg_status.mode.to_string(),
            token: mask_token(&tg.token),
            owner: tg.owner.clone(),
            last_error: tg_status.last_error,
        }],
    })
}

#[crate::instrument_api(method = "POST", path = "/api/channels/telegram/owner")]
pub(crate) async fn post_telegram_owner(
    State(state): State<AppState>,
    Json(body): Json<api_types::TelegramOwnerRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .settings
        .update_config(|cfg| {
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

    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/notify — send a Telegram notification.
///
/// The daemon itself doesn't hold a Telegram connection (that's mando-tg),
/// so we emit a bus event that TG subscribers can pick up.
#[crate::instrument_api(method = "POST", path = "/api/notify")]
pub(crate) async fn post_notify(
    State(state): State<AppState>,
    Json(body): Json<api_types::NotifyRequest>,
) -> Result<Json<api_types::NotifyResponse>, ApiError> {
    let config = state.settings.load_config();
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
    let payload = api_types::NotificationPayload {
        message: body.message.clone(),
        level: api_types::NotifyLevel::Normal,
        kind: api_types::NotificationKind::Generic,
        task_key: None,
        reply_markup: None,
    };
    state
        .bus
        .send(global_bus::BusPayload::Notification(payload));

    tracing::info!(
        module = "notify",
        chat_id = %chat_id,
        "notification emitted"
    );

    Ok(Json(api_types::NotifyResponse { ok: true, chat_id }))
}

/// POST /api/firecrawl/scrape — scrape a URL using Firecrawl API.
#[crate::instrument_api(method = "POST", path = "/api/firecrawl/scrape")]
pub(crate) async fn post_firecrawl_scrape(
    Json(body): Json<api_types::FirecrawlScrapeRequest>,
) -> Result<Json<api_types::FirecrawlScrapeResponse>, ApiError> {
    match scout::scrape_with_firecrawl(&body.url).await {
        Ok(content) => Ok(Json(api_types::FirecrawlScrapeResponse {
            ok: true,
            content,
        })),
        Err(e) => Err(internal_error(e, "firecrawl scrape failed")),
    }
}
