//! Prometheus metrics — recorder install + `/metrics` scrape endpoint +
//! HTTP request counter/duration middleware.
//!
//! Installed once at daemon startup via [`install_metrics_recorder`]; the
//! returned [`PrometheusHandle`] renders the Prometheus text exposition
//! format on demand (the `/metrics` route handler, [`render_metrics`]).
//! Axum middleware [`record_http_metrics`] wraps every request and emits
//! `http_requests_total` + `http_request_duration_seconds` with
//! `method` / `route` / `status` labels.

use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder and cache a handle for rendering.
///
/// Idempotent: repeated calls are no-ops after the first install succeeds.
/// Returns `Err` only if the global recorder was already installed by a
/// different caller (another exporter crate in the same process).
pub fn install_metrics_recorder() -> anyhow::Result<()> {
    if HANDLE.get().is_some() {
        return Ok(());
    }
    // Histogram buckets match typical Mando daemon latencies: mostly sub-ms
    // to 1s for local DB / SSE work, with a tail up to 30s for long clarify
    // and captain tick calls.
    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Full("http_request_duration_seconds".to_string()),
            &[
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
            ],
        )
        .map_err(|e| anyhow::anyhow!("configure prometheus buckets: {e}"))?
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("install prometheus recorder: {e}"))?;
    // A prior successful install would already return early above, so a
    // non-empty slot here can only come from a concurrent caller winning
    // the race. `best_effort!` records that rather than silently dropping.
    global_infra::best_effort!(
        HANDLE
            .set(handle)
            .map_err(|_| "prometheus handle already set"),
        "cache prometheus handle"
    );
    Ok(())
}

/// Axum handler for `GET /metrics` — returns Prometheus text exposition.
///
/// 503 if the recorder hasn't been installed yet (shouldn't happen in prod
/// once `install_metrics_recorder` runs at startup, but we return a
/// structured error instead of panicking if something changes that
/// ordering).
pub async fn render_metrics() -> Response {
    match HANDLE.get() {
        Some(h) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
            )],
            h.render(),
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "metrics recorder not yet installed",
        )
            .into_response(),
    }
}

/// Axum middleware — increments `http_requests_total` and records
/// `http_request_duration_seconds` for every request.
///
/// Labels: `method` (GET/POST/…), `route` (matched path pattern like
/// `/api/tasks/:id` from the axum `MatchedPath` extension), `status`
/// (3-digit HTTP status code). Must be installed via
/// `Router::route_layer` so it runs after route matching — otherwise
/// `MatchedPath` is absent and we fall back to the `"unmatched"`
/// sentinel (bounded single-series) rather than raw URI paths.
pub async fn record_http_metrics(request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status().as_u16().to_string();

    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status.clone(),
    )
    .increment(1);
    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "route" => route,
        "status" => status,
    )
    .record(duration.as_secs_f64());

    response
}
