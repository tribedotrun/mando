//! transport-http -- HTTP transport surface for the Mando daemon.

mod api_router;
mod auth;
mod config;
mod io;
mod middleware;
mod response;
mod runtime;
mod service;
mod static_files;
mod transport;
mod types;

pub use api_router::{contract_inventory_link_anchor, ApiRouter};
pub use auth::ensure_auth_token;
pub use runtime::captain_support::captain_notifier;
pub use settings::config::resolve_github_repo;
pub use transport::router::build_router;
pub use transport::*;
pub use transport_http_macros::instrument_api;
pub use types::AppState;

#[doc(hidden)]
#[macro_export]
macro_rules! __api_method_router {
    (GET, $handler:expr) => {
        axum::routing::get($handler)
    };
    (POST, $handler:expr) => {
        axum::routing::post($handler)
    };
    (PUT, $handler:expr) => {
        axum::routing::put($handler)
    };
    (PATCH, $handler:expr) => {
        axum::routing::patch($handler)
    };
    (DELETE, $handler:expr) => {
        axum::routing::delete($handler)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __api_route_method_enum {
    (GET) => {
        api_types::RouteMethod::Get
    };
    (POST) => {
        api_types::RouteMethod::Post
    };
    (PUT) => {
        api_types::RouteMethod::Put
    };
    (PATCH) => {
        api_types::RouteMethod::Patch
    };
    (DELETE) => {
        api_types::RouteMethod::Delete
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __api_optional_type_name {
    () => {
        None
    };
    ($ty:ty) => {
        Some(stringify!($ty))
    };
}

#[macro_export]
macro_rules! api_route {
    (
        $router:expr,
        $method:ident $path:literal,
        transport = Sse,
        auth = $auth:ident,
        handler = $handler:expr,
        event = $event:ty
        $(, body = $body:ty)?
        $(, query = $query:ty)?
        $(, params = $params:ty)?
        $(, res = $res:ty)?
        $(,)?
    ) => {{
        inventory::submit! {
            api_types::RouteRegistration {
                method: $crate::__api_route_method_enum!($method),
                path: $path,
                transport: api_types::RouteTransport::Sse,
                auth: api_types::RouteAuth::$auth,
                body_ty: $crate::__api_optional_type_name!($($body)?),
                query_ty: $crate::__api_optional_type_name!($($query)?),
                params_ty: $crate::__api_optional_type_name!($($params)?),
                res_ty: $crate::__api_optional_type_name!($($res)?),
                event_ty: Some(stringify!($event)),
            }
        }
        $router.route($path, $crate::__api_method_router!($method, $handler))
    }};
    (
        $router:expr,
        $method:ident $path:literal,
        transport = Json,
        auth = $auth:ident,
        handler = $handler:expr
        $(, body = $body:ty)?
        $(, query = $query:ty)?
        $(, params = $params:ty)?
        $(, res = $res:ty)?
        $(,)?
    ) => {{
        inventory::submit! {
            api_types::RouteRegistration {
                method: $crate::__api_route_method_enum!($method),
                path: $path,
                transport: api_types::RouteTransport::Json,
                auth: api_types::RouteAuth::$auth,
                body_ty: $crate::__api_optional_type_name!($($body)?),
                query_ty: $crate::__api_optional_type_name!($($query)?),
                params_ty: $crate::__api_optional_type_name!($($params)?),
                res_ty: $crate::__api_optional_type_name!($($res)?),
                event_ty: None,
            }
        }
        $router.route($path, $crate::__api_method_router!($method, $handler))
    }};
    (
        $router:expr,
        $method:ident $path:literal,
        transport = Multipart,
        auth = $auth:ident,
        handler = $handler:expr
        $(, body = $body:ty)?
        $(, query = $query:ty)?
        $(, params = $params:ty)?
        $(, res = $res:ty)?
        $(,)?
    ) => {{
        inventory::submit! {
            api_types::RouteRegistration {
                method: $crate::__api_route_method_enum!($method),
                path: $path,
                transport: api_types::RouteTransport::Multipart,
                auth: api_types::RouteAuth::$auth,
                body_ty: $crate::__api_optional_type_name!($($body)?),
                query_ty: $crate::__api_optional_type_name!($($query)?),
                params_ty: $crate::__api_optional_type_name!($($params)?),
                res_ty: $crate::__api_optional_type_name!($($res)?),
                event_ty: None,
            }
        }
        $router.route($path, $crate::__api_method_router!($method, $handler))
    }};
    (
        $router:expr,
        $method:ident $path:literal,
        transport = Ndjson,
        auth = $auth:ident,
        handler = $handler:expr
        $(, body = $body:ty)?
        $(, query = $query:ty)?
        $(, params = $params:ty)?
        $(, res = $res:ty)?
        $(,)?
    ) => {{
        inventory::submit! {
            api_types::RouteRegistration {
                method: $crate::__api_route_method_enum!($method),
                path: $path,
                transport: api_types::RouteTransport::Ndjson,
                auth: api_types::RouteAuth::$auth,
                body_ty: $crate::__api_optional_type_name!($($body)?),
                query_ty: $crate::__api_optional_type_name!($($query)?),
                params_ty: $crate::__api_optional_type_name!($($params)?),
                res_ty: $crate::__api_optional_type_name!($($res)?),
                event_ty: None,
            }
        }
        $router.route($path, $crate::__api_method_router!($method, $handler))
    }};
    (
        $router:expr,
        $method:ident $path:literal,
        transport = Static,
        auth = $auth:ident,
        handler = $handler:expr
        $(, body = $body:ty)?
        $(, query = $query:ty)?
        $(, params = $params:ty)?
        $(, res = $res:ty)?
        $(,)?
    ) => {{
        inventory::submit! {
            api_types::RouteRegistration {
                method: $crate::__api_route_method_enum!($method),
                path: $path,
                transport: api_types::RouteTransport::Static,
                auth: api_types::RouteAuth::$auth,
                body_ty: $crate::__api_optional_type_name!($($body)?),
                query_ty: $crate::__api_optional_type_name!($($query)?),
                params_ty: $crate::__api_optional_type_name!($($params)?),
                res_ty: $crate::__api_optional_type_name!($($res)?),
                event_ty: None,
            }
        }
        $router.route($path, $crate::__api_method_router!($method, $handler))
    }};
}
