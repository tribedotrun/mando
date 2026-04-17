//! API routes for credential management.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::credentials_oauth::decode_jwt_expiry;
use crate::AppState;

pub(crate) fn credential_routes() -> Router<AppState> {
    Router::new()
        .route("/api/credentials", get(list_credentials))
        .route("/api/credentials/{id}", delete(remove_credential))
        .route("/api/credentials/{id}/token", get(get_credential_token))
        .route("/api/credentials/setup-token", post(add_setup_token))
}

/// GET /api/credentials -- list all stored credentials (no secrets).
async fn list_credentials(State(state): State<AppState>) -> impl IntoResponse {
    let creds = state.credential_mgr.list().await;
    Json(serde_json::json!({ "credentials": creds }))
}

/// GET /api/credentials/:id/token -- reveal the full token.
async fn get_credential_token(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> (StatusCode, Json<serde_json::Value>) {
    match settings::io::credentials::get_token_by_id(state.db.pool(), id).await {
        Ok(Some(token)) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /api/credentials/:id -- remove a credential.
async fn remove_credential(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.credential_mgr.remove(id).await {
        Ok(true) => {
            state.bus.send(global_types::BusEvent::Credentials, None);
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "ok": false, "error": "not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        ),
    }
}

#[derive(serde::Deserialize)]
struct SetupTokenRequest {
    label: String,
    token: String,
}

/// POST /api/credentials/setup-token -- add a setup-token credential.
async fn add_setup_token(
    State(state): State<AppState>,
    Json(body): Json<SetupTokenRequest>,
) -> impl IntoResponse {
    let label = body.label.trim().to_string();
    let token = body.token.trim().to_string();
    if label.is_empty() || token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "label and token are required" })),
        );
    }

    let expires_at = decode_jwt_expiry(&token);

    match state.credential_mgr.store(&label, &token, expires_at).await {
        Ok(id) => {
            state.bus.send(global_types::BusEvent::Credentials, None);
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "ok": true, "id": id, "label": label })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
