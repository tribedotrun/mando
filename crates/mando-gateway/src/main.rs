//! mando-gw — unified Mando daemon.
//!
//! Runs axum HTTP server, captain auto-tick, and embedded Telegram bot
//! as a single process managed by macOS launchd.

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

    let auth_token = mando_gateway::auth::ensure_auth_token();

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

    // Seed projects from config.json into DB (first run only), then load
    // all projects from DB back into config so the DB is the source of truth.
    let config = {
        let mut cfg = config;
        mando_db::queries::projects::startup_sync(db.pool(), &mut cfg)
            .await
            .expect("fatal: failed to sync projects from DB");
        cfg
    };

    let config_arc = Arc::new(ArcSwap::from_pointee(config.clone()));
    let config_write_mu = Arc::new(tokio::sync::Mutex::new(()));
    let (tick_tx, _) = tokio::sync::watch::channel(
        mando_gateway::config_manager::initial_tick_duration(&config),
    );
    let config_manager = mando_gateway::config_manager::ConfigManager::new(
        config_arc.clone(),
        config_write_mu.clone(),
        tick_tx,
    );

    // Refuse to start if reconciliation fails — booting with an unreconciled
    // WAL can silently duplicate or lose external work. Set MANDO_UNSAFE_START=1
    // to override after inspecting the error.
    let unsafe_start = std::env::var("MANDO_UNSAFE_START")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    mando_gateway::startup::startup_reconciliation(db.pool()).await;

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

    let task_tracker = TaskTracker::new();
    let cancellation_token = CancellationToken::new();
    let ui_runtime = Arc::new(mando_gateway::ui_runtime::UiRuntime::new(
        mando_config::state_dir().join("ui-state.json"),
    ));
    let telegram_runtime = Arc::new(mando_gateway::telegram_runtime::TelegramRuntime::new(
        port, auth_token,
    ));

    let qa_session_mgr = mando_scout::runtime::qa::session_manager_from_workflow(&scout_wf);
    let state = mando_gateway::AppState {
        config: config_arc.clone(),
        config_manager,
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(captain_wf)),
        scout_workflow: Arc::new(ArcSwap::from_pointee(scout_wf)),
        config_write_mu,
        bus: bus.clone(),
        cc_session_mgr: Arc::new(cc_session_mgr),
        task_store: task_store_arc,
        credential_mgr: Arc::new(mando_gateway::credentials::CredentialManager::new(
            db.pool().clone(),
        )),
        db,
        qa_session_mgr,
        terminal_host: Arc::new(mando_terminal::TerminalHost::new(mando_config::data_dir())),
        start_time,
        listen_port: port,
        dev_mode: args.dev,
        task_tracker,
        cancellation_token,
        telegram_runtime,
        ui_runtime,
        scout_processing_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        auto_title_notify: Arc::new(tokio::sync::Notify::new()),
    };

    if !args.no_ui {
        state
            .ui_runtime
            .start_monitor(&state.task_tracker, state.cancellation_token.clone());
    }

    // Set up CC session hook (writes script + syncs settings.json).
    mando_gateway::hooks::setup_session_hooks();

    // Spawn captain auto-tick loop (always runs; respects auto_schedule dynamically).
    mando_gateway::background_tasks::spawn_auto_tick(&state, state.config_manager.subscribe_tick());
    mando_gateway::startup::resume_pending_scout_items(&state).await;

    // Spawn workbench cleanup (5 min after startup, removes worktrees archived > 30 days).
    mando_gateway::background_tasks::spawn_workbench_cleanup(&state);

    // Auto-title terminal workbenches from CC session content (60s loop).
    mando_gateway::auto_title::spawn(&state);

    // Auto-resume terminal sessions that were alive when the daemon last exited.
    mando_gateway::background_tasks::spawn_terminal_auto_resume(&state);

    if !args.no_telegram {
        if let Err(err) = state.telegram_runtime.configure(&config).await {
            warn!(module = "telegram", error = %err, "failed to start embedded telegram runtime");
        }
    } else {
        info!("telegram disabled via --no-telegram");
    }

    mando_gateway::instance::write_port_file(port, args.dev);

    // Auto-spawn Electron if env vars are set and UI is not disabled.
    if !args.no_ui {
        if let (Ok(electron_bin), Ok(entrypoint)) = (
            std::env::var("MANDO_ELECTRON_BIN"),
            std::env::var("MANDO_ELECTRON_ENTRYPOINT"),
        ) {
            let ui_rt = state.ui_runtime.clone();
            let headless = args.headless;
            state.task_tracker.spawn(async move {
                // Wait 3s for an existing Electron to register (e.g. first-run,
                // update relaunch, login-item launch).
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let status = ui_rt.status().await;
                if status.running {
                    info!("Electron already registered, skipping auto-spawn");
                    return;
                }

                let mut env_map = std::collections::HashMap::new();
                env_map.insert("MANDO_EXTERNAL_GATEWAY".to_string(), "1".to_string());
                env_map.insert("MANDO_GATEWAY_PORT".to_string(), port.to_string());
                if let Ok(v) = std::env::var("MANDO_AUTH_TOKEN") {
                    env_map.insert("MANDO_AUTH_TOKEN".to_string(), v);
                } else if let Ok(v) =
                    std::fs::read_to_string(mando_config::data_dir().join("auth-token"))
                {
                    env_map.insert("MANDO_AUTH_TOKEN".to_string(), v.trim().to_string());
                }
                for key in &[
                    "MANDO_APP_MODE",
                    "MANDO_DATA_DIR",
                    "MANDO_LOG_DIR",
                    "ELECTRON_DISABLE_SECURITY_WARNINGS",
                ] {
                    if let Ok(v) = std::env::var(key) {
                        env_map.insert(key.to_string(), v);
                    }
                }
                if headless {
                    env_map.insert("MANDO_HEADLESS".to_string(), "1".to_string());
                }

                let mut args = vec![entrypoint];
                if let Ok(inspect) = std::env::var("MANDO_ELECTRON_INSPECT_PORT") {
                    args.push(format!("--inspect={inspect}"));
                }
                if let Ok(cdp) = std::env::var("MANDO_ELECTRON_CDP_PORT") {
                    args.push(format!("--remote-debugging-port={cdp}"));
                }

                let spec = mando_gateway::ui_runtime::UiLaunchSpec {
                    exec_path: electron_bin,
                    args,
                    cwd: None,
                    env: env_map,
                };

                // Register the spec and launch.
                if let Err(e) = ui_rt.set_launch_spec(spec).await {
                    warn!(module = "ui", error = %e, "failed to set launch spec for auto-spawn");
                    return;
                }
                if let Err(e) = ui_rt.launch().await {
                    warn!(module = "ui", error = %e, "failed to auto-spawn Electron");
                }
            });
        }
    }

    let qa_mgr = state.qa_session_mgr.clone();
    let tg_rt = state.telegram_runtime.clone();
    let ui_rt = state.ui_runtime.clone();
    let terminal_host = state.terminal_host.clone();
    let tracker = state.task_tracker.clone();
    let cancel = state.cancellation_token.clone();
    let app = mando_gateway::server::build_router(state);
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .expect("invalid listen address");

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
        cancel.cancel();
        tg_rt.shutdown().await;
        ui_rt.shutdown().await;
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("server error");

    // Drain all tracked spawns before tearing down the runtime so fire-and-
    // forget work gets a clean exit.
    tracker.close();
    tracker.wait().await;

    // Kill all terminal PTY sessions.
    terminal_host.shutdown();

    // Shut down persistent Q&A sessions (kills CC child processes).
    qa_mgr.shutdown().await;

    mando_gateway::telemetry::shutdown_tracing();

    mando_gateway::instance::cleanup_files(args.dev);
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
        let socket = tokio::net::TcpSocket::new_v4().expect("socket creation failed");
        socket.set_reuseaddr(true).expect("SO_REUSEADDR failed");

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
