//! Generated typed client for the Mando daemon.
//!
//! Route descriptors and methods are generated from the server's
//! `api_route!` registry. Rust callers choose a generated method rather than
//! supplying an HTTP method, path, request type, or response type themselves.

mod generated;
mod http;
mod sse;

pub use generated::routes;
pub use http::{ClientError, GatewayClient, Result, RouteDescriptor};
pub use sse::{parse_sse_block, ParseError, SseConsumer, SseSignal};
