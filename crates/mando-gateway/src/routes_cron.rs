//! /api/cron/* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Gate: return 503 if the cron feature is disabled.
fn require_cron(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    if !state.config.load().features.cron {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "cron is disabled",
        ));
    }
    Ok(())
}

/// GET /api/cron
pub(crate) async fn get_cron(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_cron(&state)?;

    let svc = state.cron_service.read().await;
    Ok(Json(mando_shared::list_cron_jobs(&svc, true)))
}

#[derive(Deserialize)]
pub(crate) struct AddCronBody {
    pub name: String,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub message: String,
}

/// POST /api/cron/add
pub(crate) async fn post_cron_add(
    State(state): State<AppState>,
    Json(body): Json<AddCronBody>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_cron(&state)?;

    let schedule = mando_shared::parse_schedule(&body.schedule_kind, &body.schedule_value)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e))?;

    let id = format!("cron-{}", now_ms());

    let mut svc = state.cron_service.write().await;
    match mando_shared::add_cron_job(&mut svc, id, body.name, schedule, body.message, now_ms())
        .await
    {
        Ok(val) => {
            state
                .bus
                .send(mando_types::BusEvent::Cron, Some(json!({"action": "add"})));
            Ok((StatusCode::CREATED, Json(val)))
        }
        Err(e) => Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, &e)),
    }
}

#[derive(Deserialize)]
pub(crate) struct CronIdBody {
    pub id: String,
}

/// POST /api/cron/remove
pub(crate) async fn post_cron_remove(
    State(state): State<AppState>,
    Json(body): Json<CronIdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_cron(&state)?;

    let mut svc = state.cron_service.write().await;
    match mando_shared::remove_cron_job(&mut svc, &body.id).await {
        Ok(val) => {
            state.bus.send(
                mando_types::BusEvent::Cron,
                Some(json!({"action": "remove"})),
            );
            Ok(Json(val))
        }
        Err(e) => Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, &e)),
    }
}

#[derive(Deserialize)]
pub(crate) struct ToggleBody {
    pub id: String,
    pub enabled: bool,
}

/// POST /api/cron/toggle
pub(crate) async fn post_cron_toggle(
    State(state): State<AppState>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_cron(&state)?;

    let mut svc = state.cron_service.write().await;
    match mando_shared::toggle_cron_job(&mut svc, &body.id, body.enabled).await {
        Ok(val) => {
            state.bus.send(
                mando_types::BusEvent::Cron,
                Some(json!({"action": "toggle"})),
            );
            Ok(Json(val))
        }
        Err(e) => Err(error_response(StatusCode::NOT_FOUND, &e)),
    }
}

/// POST /api/cron/run
pub(crate) async fn post_cron_run(
    State(state): State<AppState>,
    Json(body): Json<CronIdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_cron(&state)?;

    let mut svc = state.cron_service.write().await;
    match svc.run_job(&body.id).await {
        Ok(_) => {
            state.bus.send(
                mando_types::BusEvent::Cron,
                Some(json!({"action": "run", "id": body.id})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(error_response(StatusCode::NOT_FOUND, &e)),
    }
}

use mando_shared::cron::service::now_ms;
