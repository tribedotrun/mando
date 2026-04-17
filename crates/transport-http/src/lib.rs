//! transport-http -- shared HTTP infrastructure for the Mando daemon.
//!
//! Contains auth, middleware, response helpers, and static file serving.
//! Route handlers live in mando-gateway (coupled to AppState).

pub mod auth;
pub mod middleware;
pub mod response;
pub mod static_files;
