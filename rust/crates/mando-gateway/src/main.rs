//! mando-gw — unified Mando daemon.
//!
//! Runs axum HTTP server, captain auto-tick, and embedded Telegram bot
//! as a single process managed by macOS launchd.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "mando-gw", about = "Mando daemon — HTTP API + captain")]
struct Args {
    /// Port to listen on (overrides config)
    #[arg(short = 'p', long)]
    port: Option<u16>,

    /// Run in foreground (logs to stderr instead of file)
    #[arg(long)]
    foreground: bool,

    /// Dev mode — writes daemon-dev.port instead of daemon.port.
    /// Forces all models to sonnet. Mutually exclusive with --sandbox.
    #[arg(long, conflicts_with = "sandbox")]
    dev: bool,

    /// Sandbox mode — forces all models (captain, worker, clarifier, scout,
    /// terminal sessions) to haiku so tests are fast and cheap.
    /// Mutually exclusive with --dev.
    #[arg(long)]
    sandbox: bool,

    /// No Electron UI at all (no window, no CDP)
    #[arg(long)]
    no_ui: bool,

    /// Spawn Electron but invisible (no window, no Dock icon). CDP works.
    #[arg(long)]
    headless: bool,

    /// Skip embedded Telegram bot
    #[arg(long)]
    no_telegram: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    mando_gateway::init_tracing(args.foreground);

    // Install the global panic hook immediately after the tracing subscriber
    // is ready. Enforces PR #883 invariant #1 (no silent crashes): any panic
    // from this point on lands a structured log line with message, location,
    // and backtrace before Rust's default hook prints to stderr.
    global_infra::install_panic_hook();

    let start_time = Instant::now();

    // Load config.
    let config = settings::config_fs::load_config(None).unwrap_or_else(|e| {
        eprintln!("fatal: failed to load config: {e}");
        std::process::exit(1);
    });
    // Inject env vars from config into process environment.
    for (k, v) in &config.env {
        // SAFETY: single-threaded at this point, before any spawns.
        unsafe { std::env::set_var(k, v) };
    }

    let port = args.port.unwrap_or(config.gateway.dashboard.port);

    if let Err(e) = mando_gateway::check_and_write_pid(port) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }

    // Sync bundled prod skills to ~/.claude/skills/mando-*.
    settings::config::skills::sync_bundled_skills();

    // Refuse to start if reconciliation fails — booting with an unreconciled
    // WAL can silently duplicate or lose external work. Set MANDO_UNSAFE_START=1
    // to override after inspecting the error.
    let unsafe_start = std::env::var("MANDO_UNSAFE_START")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let bootstrap = mando_gateway::bootstrap_gateway(
        config,
        Arc::new(global_bus::EventBus::new()),
        mando_gateway::BootstrapOptions {
            port_override: Some(port),
            dev_mode: args.dev,
            sandbox_mode: args.sandbox,
            unsafe_start,
            start_time,
        },
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("fatal: bootstrap failed: {e}");
        std::process::exit(1);
    });
    let mando_gateway::GatewayBootstrap { state, .. } = bootstrap;

    mando_gateway::start_runtime_services(
        &state,
        mando_gateway::RuntimeStartOptions {
            start_ui_monitor: !args.no_ui,
            start_telegram: !args.no_telegram,
        },
    )
    .await;
    state
        .captain
        .drain_pending_lifecycle_effects()
        .await
        .unwrap_or_else(|e| {
            eprintln!("fatal: failed to drain lifecycle effects: {e}");
            std::process::exit(1);
        });

    // Set up CC session hook (writes script + syncs settings.json).
    mando_gateway::setup_session_hooks();

    mando_gateway::write_port_file(port, args.dev);

    // Auto-spawn Electron if env vars are set and UI is not disabled.
    if !args.no_ui {
        if let (Ok(electron_bin), Ok(entrypoint)) = (
            std::env::var("MANDO_ELECTRON_BIN"),
            std::env::var("MANDO_ELECTRON_ENTRYPOINT"),
        ) {
            match resolve_daemon_auth_token() {
                Some(auth_token) => state.ui_runtime.schedule_daemon_autolaunch(
                    &state.task_tracker,
                    state.cancellation_token.clone(),
                    transport_ui::UiAutoLaunchOptions {
                        exec_path: electron_bin,
                        entrypoint,
                        gateway_port: port,
                        auth_token,
                        headless: args.headless,
                        app_mode: std::env::var("MANDO_APP_MODE").ok(),
                        data_dir: std::env::var("MANDO_DATA_DIR").ok(),
                        log_dir: std::env::var("MANDO_LOG_DIR").ok(),
                        disable_security_warnings: std::env::var(
                            "ELECTRON_DISABLE_SECURITY_WARNINGS",
                        )
                        .ok(),
                        inspect_port: std::env::var("MANDO_ELECTRON_INSPECT_PORT").ok(),
                        cdp_port: std::env::var("MANDO_ELECTRON_CDP_PORT").ok(),
                    },
                ),
                None => warn!(
                    module = "ui",
                    "failed to resolve auth token for Electron auto-spawn"
                ),
            }
        }
    }

    let qa_mgr = state.scout.qa_session_mgr().clone();
    let tg_rt = state.telegram_runtime.clone();
    let ui_rt = state.ui_runtime.clone();
    let terminal = state.terminal.clone();
    let tracker = state.task_tracker.clone();
    let cancel = state.cancellation_token.clone();
    let app = mando_gateway::build_router(state);
    let addr: SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(a) => a,
        Err(e) => global_infra::unrecoverable!("invalid listen address", e),
    };

    let listener = bind_with_retry(addr).await;

    info!("mando-gw listening on {addr}");

    // Shutdown order (per plan):
    // 1. Receive signal, cancel cooperative loops
    // 2. Shutdown TG (needs service layer alive for in-flight updates)
    // 3. Shutdown UI (SIGTERM Electron, wait up to 5s)
    // 4. Close HTTP server (drain in-flight requests)
    // 5. Drain tracked spawns
    // 6. Exit
    //
    // TG and UI shutdown happen INSIDE the graceful_shutdown closure so they
    // run before axum tries to drain SSE connections (Electron holds an SSE
    // connection that would block drain indefinitely if we killed it after).
    let shutdown = async move {
        shutdown_signal().await;
        // Cancel cooperative loops FIRST so auto-tick and request handlers
        // stop spawning new CC sessions. If we snapshotted before cancel,
        // the 5s grace window that signal_cc_subprocesses_for_shutdown
        // spends waiting for exits would let brand-new sessions slip past
        // — their PIDs would not be in our snapshot, and they would
        // become orphans the moment the daemon exits.
        cancel.cancel();
        // Yield once so the cancellation propagates to any task that is
        // mid-spawn (about to register a PID). After this, the registry
        // snapshot reflects every session that was ever registered.
        tokio::task::yield_now().await;
        // Signal every live CC subprocess before tearing down tokio so
        // subprocesses get a chance to flush their final `result` event
        // to the stream. Any stragglers beyond the grace window are
        // cleaned up by `pid_registry::cleanup_on_startup` on the next
        // launch.
        mando_gateway::signal_cc_subprocesses_for_shutdown().await;
        tg_rt.shutdown().await;
        ui_rt.shutdown().await;
    };
    // Capture any serve-side failure but don't short-circuit cleanup —
    // we still need tracker/terminal/qa_mgr shutdown to flush state
    // before the process leaves. The exit-code decision is deferred to
    // the very end so launchd / supervising processes still see a
    // non-zero status when the HTTP server errored unexpectedly.
    let serve_error = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .err();
    if let Some(ref e) = serve_error {
        tracing::error!(target: "mando-gateway", module = "mando-gateway", %e, "axum server exited with error");
    }

    // Drain all tracked spawns before tearing down the runtime so fire-and-
    // forget work gets a clean exit.
    tracker.close();
    tracker.wait().await;

    // Kill all terminal PTY sessions.
    terminal.shutdown();

    // Shut down persistent Q&A sessions (kills CC child processes).
    qa_mgr.shutdown().await;

    mando_gateway::shutdown_tracing();

    mando_gateway::cleanup_files(args.dev);
    if serve_error.is_some() {
        // All cleanup has already run above; surfacing the non-zero exit
        // is the last step so the supervising process sees the failure.
        info!("mando-gw shutdown complete (exit=1 due to serve error)");
        std::process::exit(1);
    }
    info!("mando-gw shutdown complete");
}

/// Bind with retry and exponential backoff (1s, 2s, 4s, 8s, 16s).
///
/// Sets SO_REUSEADDR so the new process can bind even when the previous socket
/// is still in TIME_WAIT.  Does NOT set SO_REUSEPORT -- that would let two
/// daemons bind the same port simultaneously (kernel load-balances between
/// them), defeating single-instance enforcement.  If all attempts fail, exits
/// cleanly (no panic) so launchd can retry without a crash-loop counter.
async fn bind_with_retry(addr: SocketAddr) -> tokio::net::TcpListener {
    const MAX_ATTEMPTS: u32 = 5;

    for attempt in 1..=MAX_ATTEMPTS {
        let socket = match tokio::net::TcpSocket::new_v4() {
            Ok(s) => s,
            Err(e) => global_infra::unrecoverable!("socket creation failed", e),
        };
        if let Err(e) = socket.set_reuseaddr(true) {
            global_infra::unrecoverable!("SO_REUSEADDR failed", e);
        }

        match socket.bind(addr) {
            Ok(()) => match socket.listen(1024) {
                Ok(listener) => return listener,
                Err(e) => {
                    if attempt == MAX_ATTEMPTS {
                        eprintln!(
                            "fatal: listen failed on {addr} after {MAX_ATTEMPTS} attempts: {e}"
                        );
                        std::process::exit(1);
                    }
                    let backoff_s = 1u64 << (attempt - 1);
                    warn!(
                        attempt,
                        max = MAX_ATTEMPTS,
                        backoff_s,
                        "listen on {addr} failed ({e}), retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                }
            },
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    eprintln!("fatal: bind to {addr} failed after {MAX_ATTEMPTS} attempts: {e}");
                    std::process::exit(1);
                }
                let backoff_s = 1u64 << (attempt - 1);
                warn!(
                    attempt,
                    max = MAX_ATTEMPTS,
                    backoff_s,
                    "bind to {addr} failed ({e}), retrying"
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
            }
        }
    }

    unreachable!()
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => global_infra::unrecoverable!("install SIGTERM handler", e),
            };
        tokio::select! {
            _ = ctrl_c => { info!("received SIGINT"); }
            _ = sigterm.recv() => { info!("received SIGTERM"); }
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.expect("ctrl-c handler");
        info!("received SIGINT");
    }
}

fn resolve_daemon_auth_token() -> Option<String> {
    std::env::var("MANDO_AUTH_TOKEN").ok().or_else(|| {
        std::fs::read_to_string(global_infra::paths::data_dir().join("auth-token"))
            .ok()
            .map(|value| value.trim().to_string())
    })
}
