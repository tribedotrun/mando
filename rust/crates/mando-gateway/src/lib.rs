//! mando-gateway — daemon composition root for bootstrap, startup, and shutdown.

mod bootstrap;
mod drift_test;
mod hooks;
mod instance;
mod server;
mod session_backend;
mod shutdown;
mod startup;
mod telemetry;

pub use bootstrap::{
    bootstrap_gateway, start_runtime_services, BootstrapOptions, GatewayBootstrap,
    RuntimeStartOptions,
};
pub use hooks::setup_session_hooks;
pub use instance::{check_and_write_pid, cleanup_files, write_port_file};
pub use server::{build_router, start_server};
pub use shutdown::signal_cc_subprocesses_for_shutdown;
pub use telemetry::{init_tracing, shutdown_tracing};
pub use transport_http::contract_inventory_link_anchor;
pub use transport_http::AppState;
