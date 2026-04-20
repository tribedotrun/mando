use std::future::Future;
use std::sync::Arc;

use tracing::info;

pub use transport_http::build_router;

pub async fn start_server(
    cfg: settings::config::Config,
    bus: global_bus::EventBus,
) -> anyhow::Result<()> {
    start_server_with(cfg, bus, std::future::pending::<()>(), false).await
}

/// Full `start_server` with explicit shutdown signal and unsafe-start gate.
pub async fn start_server_with<F>(
    config: settings::config::Config,
    bus: global_bus::EventBus,
    shutdown: F,
    unsafe_start: bool,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let bootstrap = crate::bootstrap::bootstrap_gateway(
        config,
        Arc::new(bus),
        crate::bootstrap::BootstrapOptions {
            port_override: None,
            dev_mode: false,
            sandbox_mode: false,
            unsafe_start,
            start_time: std::time::Instant::now(),
        },
    )
    .await?;

    let crate::bootstrap::GatewayBootstrap { state, host } = bootstrap;
    let port = state.listen_port;

    crate::bootstrap::start_runtime_services(
        &state,
        crate::bootstrap::RuntimeStartOptions {
            start_ui_monitor: true,
            start_telegram: true,
        },
    )
    .await;
    state.captain.drain_pending_lifecycle_effects().await?;

    let terminal = state.terminal.clone();
    let tracker = state.task_tracker.clone();
    let cancel = state.cancellation_token.clone();
    let app = build_router(state);
    let addr = format!("{host}:{port}");
    info!(module = "mando-gateway", "gateway listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let graceful = async move {
        shutdown.await;
        cancel.cancel();
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await?;

    terminal.shutdown();
    tracker.close();
    tracker.wait().await;
    Ok(())
}

#[cfg(test)]
mod tests;
