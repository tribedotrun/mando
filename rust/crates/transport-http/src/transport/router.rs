use axum::routing::get;
use axum::{middleware, Router};

use crate::auth;
use crate::config::router::cors_layer;
use crate::middleware::{metrics, request_id};
use crate::transport::server_routes_core;
use crate::types::AppState;

pub fn build_router(state: AppState) -> Router {
    let router = server_routes_core::public_routes()
        .into_router()
        // Public, unauthenticated Prometheus scrape endpoint. Bound to
        // the daemon's loopback-only listener, so exposure is local-only.
        // Deliberately outside the api_route! typed-wire contract since
        // the response is the Prometheus text exposition format, not JSON.
        .route("/metrics", get(metrics::render_metrics))
        .merge(
            server_routes_core::protected_routes()
                .into_router()
                .route_layer(middleware::from_fn(auth::require_auth)),
        )
        .with_state(state);
    with_route_observability(router)
        .layer(middleware::from_fn(request_id::inject_request_id))
        .layer(cors_layer())
}

fn with_route_observability<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        // `.route_layer` (not `.layer`) — observability middleware must run
        // *inside* the Router's route-matching step so that the axum
        // `MatchedPath` extension is populated. With `.layer()` the
        // middleware runs before matching, and every request would
        // fall back to a raw URI label (unbounded cardinality on
        // dynamic segments like `/api/tasks/:id`). As a side effect
        // this also skips metrics for bare-miss 404s — fine, since
        // those URIs have no bounded label anyway.
        .route_layer(middleware::from_fn(metrics::record_http_metrics))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_route_observability_is_mounted_by_the_router_helper() {
        let router = Router::<()>::new().route("/test", get(|| async { "ok" }));
        let _: Router<()> = with_route_observability(router);
    }
}
