//! Request ID middleware — assigns a unique ID to each HTTP request.
//!
//! Generates a UUID v4 for each incoming request and:
//! 1. Creates a tracing span with `request_id` as a field
//! 2. Adds an `x-request-id` response header
//!
//! The span propagates through all downstream handler and service calls,
//! ensuring every log line within a request is correlated.

use axum::http::HeaderValue;
use tracing::Instrument;

/// Axum middleware function that injects a per-request trace ID.
pub async fn inject_request_id(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let request_id = global_infra::uuid::Uuid::v4().to_string();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
    );

    async move {
        let start = std::time::Instant::now();
        let mut response = next.run(request).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status().as_u16();
        tracing::info!(status, duration_ms, "request completed");
        // UUID v4 strings are always valid header values; expect is safe here.
        let val = HeaderValue::from_str(&request_id).expect("UUID is valid header value");
        response.headers_mut().insert("x-request-id", val);
        response
    }
    .instrument(span)
    .await
}
