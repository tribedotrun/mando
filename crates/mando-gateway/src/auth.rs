//! Auth-token management for the daemon.
//!
//! Generates a random bearer token on first run and persists it to
//! `~/.mando/state/auth-token`. Subsequent starts re-use the same token.

use std::fs;
use std::path::PathBuf;

/// Return the path to the persisted auth token file.
fn token_path() -> PathBuf {
    mando_config::data_dir().join("auth-token")
}

/// Ensure the auth-token file exists. Creates it with a random value if
/// missing. Returns the token string.
pub fn ensure_auth_token() -> String {
    let path = token_path();
    if let Ok(existing) = fs::read_to_string(&path) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    // Generate a random token (UUID v4, no hyphens for compactness).
    let token = mando_uuid::Uuid::v4().to_string();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create auth-token parent directory");
    }
    fs::write(&path, &token).expect("failed to write auth-token file");

    // Restrict permissions to owner-only (0600).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .expect("failed to set auth-token file permissions to 0600");
    }

    token
}

/// Read the current auth token, if it exists.
pub(crate) fn read_auth_token() -> Option<String> {
    fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn auth_not_configured_response() -> axum::response::Response {
    use axum::response::IntoResponse;

    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(serde_json::json!({"error": "auth not configured"})),
    )
        .into_response()
}

fn unauthorized_response() -> axum::response::Response {
    use axum::response::IntoResponse;

    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "unauthorized"})),
    )
        .into_response()
}

#[allow(clippy::result_large_err)]
fn expected_token() -> Result<String, axum::response::Response> {
    let expected = match read_auth_token() {
        Some(t) => t,
        None => {
            tracing::error!("auth-token file missing or unreadable — rejecting request");
            return Err(auth_not_configured_response());
        }
    };
    Ok(expected)
}

fn has_valid_bearer_token(req: &axum::http::Request<axum::body::Body>, expected: &str) -> bool {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|val| val.strip_prefix("Bearer ").map(|t| t.trim()) == Some(expected))
}

fn has_valid_query_token(req: &axum::http::Request<axum::body::Body>, expected: &str) -> bool {
    let Some(query) = req.uri().query() else {
        return false;
    };
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix("token=") {
            if val == expected {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::result_large_err)]
fn authorize_request(
    req: &axum::http::Request<axum::body::Body>,
    allow_query_token: bool,
) -> Result<(), axum::response::Response> {
    let expected = expected_token()?;

    if has_valid_bearer_token(req, &expected) {
        return Ok(());
    }

    if allow_query_token && has_valid_query_token(req, &expected) {
        return Ok(());
    }

    Err(unauthorized_response())
}

/// Axum middleware: require `Authorization: Bearer <token>`.
pub(crate) async fn require_auth(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Err(response) = authorize_request(&req, false) {
        return response;
    }
    next.run(req).await
}

/// Axum middleware for SSE endpoints that still need `?token=...` support.
///
/// This must only be attached to explicit SSE routes such as `/api/events`.
pub(crate) async fn require_sse_auth(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Err(response) = authorize_request(&req, true) {
        return response;
    }
    next.run(req).await
}
