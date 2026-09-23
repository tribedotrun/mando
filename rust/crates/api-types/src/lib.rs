//! api-types -- HTTP request/response contract for the Mando daemon API.
//!
//! Shared between transport-http (server) and Electron codegen.

mod config;
mod credential_routing;
mod credentials;
mod credentials_claude_desktop;
mod credentials_codex;
mod events;
mod extras;
mod models;
mod models_wire;
mod requests;
mod requests_extra;
mod responses;
mod responses_daemon;
mod routes;
mod sessions;
mod timeline_payload;
mod transcript_events;

pub use config::*;
pub use credential_routing::*;
pub use credentials::*;
pub use credentials_claude_desktop::*;
pub use credentials_codex::*;
pub use events::*;
pub use extras::*;
pub use models::*;
pub use models_wire::*;
pub use requests::*;
pub use requests_extra::*;
pub use responses::*;
pub use responses_daemon::*;
pub use routes::{route_registrations, RouteAuth, RouteMethod, RouteRegistration, RouteTransport};
pub use sessions::*;
pub use timeline_payload::*;
pub use transcript_events::*;

pub use global_types::{ExecutionAdapter, TaskOwnerProvider, TaskProvider};
