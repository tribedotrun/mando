//! Request ID middleware — assigns a unique ID to each HTTP request.
//!
//! Generates a UUID v4 for each incoming request and:
//! 1. Adds the ID to request extensions for the matched-route span
//! 2. Adds an `x-request-id` response header
//!
//! The router-owned observability middleware reads the extension and creates
//! the request's single tracing span.

use axum::http::HeaderValue;

#[derive(Clone)]
pub(crate) struct RequestId(pub String);

/// Axum middleware function that injects a per-request trace ID.
pub async fn inject_request_id(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let request_id = global_infra::uuid::Uuid::v4().to_string();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    // UUID v4 strings are ASCII and always legal as header values.
    // Skip the header on the unreachable failure path rather than panic.
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }
    response
}
