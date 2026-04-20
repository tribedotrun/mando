use axum::{middleware, Router};

use crate::auth;
use crate::config::router::cors_layer;
use crate::middleware::request_id;
use crate::transport::server_routes_core;
use crate::types::AppState;

pub fn build_router(state: AppState) -> Router {
    server_routes_core::public_routes()
        .into_router()
        .merge(
            server_routes_core::protected_routes()
                .into_router()
                .route_layer(middleware::from_fn(auth::require_auth)),
        )
        .layer(middleware::from_fn(request_id::inject_request_id))
        .layer(cors_layer())
        .with_state(state)
}
