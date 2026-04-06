//! mando-gw — standalone Mando daemon.
//!
//! Runs axum HTTP server and captain auto-tick as a single process
//! managed by macOS launchd. Telegram bots run separately via `mando-tg`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use clap::Parser;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
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

    /// Dev mode — writes daemon-dev.port instead of daemon.port
    #[arg(long)]
    dev: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    mando_gateway::telemetry::init_tracing(args.foreground);
    let start_time = Instant::now();

    // Load config.
    let config = mando_config::load_config(None).unwrap_or_else(|e| {
        eprintln!("fatal: failed to load config: {e}");
        std::process::exit(1);
    });
    let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
    mando_config::set_active_captain_runtime_paths(runtime_paths.clone());

    // Inject env vars from config into process environment.
    for (k, v) in &config.env {
        // SAFETY: single-threaded at this point, before any spawns.
        unsafe { std::env::set_var(k, v) };
    }

    let port = args.port.unwrap_or(config.gateway.dashboard.port);

    if let Err(e) = mando_gateway::instance::check_and_write_pid(port) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }

    mando_gateway::auth::ensure_auth_token();

    // Sync bundled prod skills to ~/.claude/skills/mando-*.
    mando_config::skills::sync_bundled_skills();

    let bus = Arc::new(mando_shared::EventBus::new());

    // Unified DB pool — shared across all subsystems.
    let db_path = runtime_paths.task_db_path.clone();
    let db = mando_db::Db::open(&db_path).await.unwrap_or_else(|e| {
        eprintln!("fatal: cannot open database at {}: {e}", db_path.display());
        std::process::exit(1);
    });
    let db = Arc::new(db);

    // Task store wraps the same pool.
    let task_store = mando_captain::io::task_store::TaskStore::new(db.pool().clone());
    let task_store_arc = Arc::new(RwLock::new(task_store));

    let config_arc = Arc::new(ArcSwap::from_pointee(config.clone()));

    // Refuse to start if reconciliation fails — booting with an unreconciled
    // WAL can silently duplicate or lose external work. Set MANDO_UNSAFE_START=1
    // to override after inspecting the error.
    let unsafe_start = std::env::var("MANDO_UNSAFE_START")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if let Err(e) =
        mando_captain::runtime::reconciler::reconcile_on_startup(&config, db.pool()).await
    {
        if unsafe_start {
            tracing::error!(
                module = "startup",
                error = %e,
                "reconciliation failed — continuing under MANDO_UNSAFE_START"
            );
        } else {
            tracing::error!(
                module = "startup",
                error = %e,
                "reconciliation failed — refusing to start (set MANDO_UNSAFE_START=1 to override)"
            );
            eprintln!("fatal: reconciliation failed: {e}");
            std::process::exit(1);
        }
    }

    let mut captain_wf = mando_config::load_captain_workflow(
        &mando_config::captain_workflow_path(),
        config.captain.tick_interval_s,
    )
    .unwrap_or_else(|e| {
        eprintln!("fatal: failed to load captain workflow: {e}");
        std::process::exit(1);
    });
    let mut scout_wf =
        mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &config)
            .unwrap_or_else(|e| {
                eprintln!("fatal: failed to load scout workflow: {e}");
                std::process::exit(1);
            });
    if args.dev {
        mando_gateway::apply_dev_model_overrides(&mut captain_wf, &mut scout_wf);
    }

    let cc_state_dir = mando_config::state_dir().join("ops_sessions").join("cc");
    let cc_session_mgr = mando_captain::io::cc_session::CcSessionManager::new(
        cc_state_dir,
        "sonnet",
        db.pool().clone(),
    );

    let cc_recovered = cc_session_mgr.recover();
    if cc_recovered.recovered > 0 || cc_recovered.corrupt > 0 {
        info!(
            recovered = cc_recovered.recovered,
            corrupt = cc_recovered.corrupt,
            "recovered sessions from disk"
        );
    }

    let state = mando_gateway::AppState {
        config: config_arc.clone(),
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(captain_wf)),
        scout_workflow: Arc::new(ArcSwap::from_pointee(scout_wf)),
        config_write_mu: Arc::new(tokio::sync::Mutex::new(())),
        bus: bus.clone(),
        cc_session_mgr: Arc::new(cc_session_mgr),
        task_store: task_store_arc,
        db,
        qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
        start_time,
        dev_mode: args.dev,
        task_tracker: TaskTracker::new(),
        cancellation_token: CancellationToken::new(),
    };

    // Spawn captain auto-tick loop (always runs; respects auto_schedule dynamically).
    let raw_tick = config.captain.tick_interval_s;
    let tick_interval_s = raw_tick.max(10);
    if raw_tick != tick_interval_s {
        tracing::warn!(
            module = "startup",
            configured = raw_tick,
            clamped = tick_interval_s,
            "tick_interval_s below minimum 10s, clamped"
        );
    }
    mando_gateway::background_tasks::spawn_auto_tick(&state, tick_interval_s);

    mando_gateway::instance::write_port_file(port, args.dev);

    let qa_mgr = state.qa_session_mgr.clone();
    let tracker = state.task_tracker.clone();
    let cancel = state.cancellation_token.clone();
    let app = mando_gateway::server::build_router(state);
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .expect("invalid listen address");

    let listener = bind_with_retry(addr).await;

    info!("mando-gw listening on {addr}");

    // Fire the cancellation token as soon as a shutdown signal arrives so
    // cooperative loops (auto-tick, SSE readers) can exit promptly.
    let shutdown = async move {
        shutdown_signal().await;
        cancel.cancel();
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("server error");

    // Drain all tracked spawns before tearing down the runtime so fire-and-
    // forget work gets a clean exit.
    tracker.close();
    tracker.wait().await;

    // Shut down persistent Q&A sessions (kills CC child processes).
    qa_mgr.shutdown().await;

    mando_gateway::telemetry::shutdown_tracing();

    mando_gateway::instance::cleanup_files(args.dev);
    info!("mando-gw shutdown complete");
}

/// Bind with retry and exponential backoff (1s, 2s, 4s, 8s, 16s).
///
/// Sets SO_REUSEADDR + SO_REUSEPORT so the new process can bind even when the
/// previous socket is still in TIME_WAIT.  If all attempts fail, exits cleanly
/// (no panic) so launchd can retry without a crash-loop counter.
async fn bind_with_retry(addr: SocketAddr) -> tokio::net::TcpListener {
    const MAX_ATTEMPTS: u32 = 5;

    for attempt in 1..=MAX_ATTEMPTS {
        let socket = tokio::net::TcpSocket::new_v4().expect("socket creation failed");
        socket.set_reuseaddr(true).expect("SO_REUSEADDR failed");
        #[cfg(unix)]
        socket.set_reuseport(true).expect("SO_REUSEPORT failed");

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
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
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
