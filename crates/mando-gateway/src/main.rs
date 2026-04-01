//! mando-gw — standalone Mando daemon.
//!
//! Runs axum HTTP server and captain auto-tick as a single process
//! managed by macOS launchd. Telegram bots run separately via `mando-tg`.

use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use clap::Parser;
use tokio::sync::RwLock;
use tracing::info;

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
    let config = mando_config::load_config(None);
    let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
    mando_config::set_active_captain_runtime_paths(runtime_paths.clone());

    // Inject env vars from config into process environment.
    for (k, v) in &config.env {
        // SAFETY: single-threaded at this point, before any spawns.
        unsafe { std::env::set_var(k, v) };
    }

    let port = args.port.unwrap_or(config.gateway.dashboard.port);

    if let Err(e) = mando_gateway::instance::check_and_write_pid() {
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

    // Cron service: wire callback before start (arm_timer skips if no callback).
    let mut cron_service = mando_shared::CronService::new(db.pool().clone());
    cron_service.set_on_job(mando_gateway::cron_executor::make_cron_callback(
        config_arc.clone(),
        task_store_arc.clone(),
        bus.clone(),
    ));
    cron_service.start().await;

    if let Err(e) =
        mando_captain::runtime::reconciler::reconcile_on_startup(&config, db.pool()).await
    {
        tracing::error!(module = "startup", error = %e, "reconciliation failed");
    }

    let captain_wf = mando_config::load_captain_workflow(
        &mando_config::captain_workflow_path(),
        config.captain.tick_interval_s,
    );
    let scout_wf = mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &config);
    let voice_wf = mando_config::load_voice_workflow(&mando_config::voice_workflow_path());

    let cc_state_dir = mando_config::state_dir().join("ops_sessions").join("cc");
    let mut cc_session_mgr = mando_captain::io::cc_session::CcSessionManager::new(
        cc_state_dir,
        "sonnet",
        db.pool().clone(),
    );

    let cc_recovered = cc_session_mgr.recover();
    if cc_recovered > 0 {
        info!(cc = cc_recovered, "recovered sessions from disk");
    }

    let state = mando_gateway::AppState {
        config: config_arc.clone(),
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(captain_wf)),
        scout_workflow: Arc::new(ArcSwap::from_pointee(scout_wf)),
        voice_workflow: Arc::new(ArcSwap::from_pointee(voice_wf)),
        config_write_mu: Arc::new(tokio::sync::Mutex::new(())),
        bus: bus.clone(),
        cron_service: Arc::new(RwLock::new(cron_service)),
        cc_session_mgr: Arc::new(RwLock::new(cc_session_mgr)),
        task_store: task_store_arc,
        db,
        linear_workspace_slug: Arc::new(RwLock::new(None)),
        qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
        start_time,
    };

    // Fetch Linear workspace slug in background.
    mando_gateway::spawn_linear_slug_fetch(
        state.config.clone(),
        state.linear_workspace_slug.clone(),
    );

    // Spawn captain auto-tick loop (always runs; respects auto_schedule dynamically).
    let tick_interval_s = config.captain.tick_interval_s.max(10);
    mando_gateway::background_tasks::spawn_auto_tick(&state, tick_interval_s);

    // Spawn distiller cron loop (always runs; respects auto_schedule dynamically).
    let learn_cron_expr = config.captain.learn_cron_expr.clone();
    mando_gateway::background_tasks::spawn_distiller_cron(
        state.config.clone(),
        state.captain_workflow.clone(),
        bus.clone(),
        state.db.pool().clone(),
        &learn_cron_expr,
    );

    mando_gateway::instance::write_port_file(port, args.dev);

    let qa_mgr = state.qa_session_mgr.clone();
    let app = mando_gateway::server::build_router(state);
    let addr = format!("127.0.0.1:{port}");

    info!("mando-gw listening on {addr}");

    let socket = tokio::net::TcpSocket::new_v4().expect("socket creation failed");
    socket.set_reuseaddr(true).expect("SO_REUSEADDR failed");
    socket
        .bind(addr.parse().expect("invalid listen address"))
        .expect("bind failed");
    let listener = socket.listen(1024).expect("listen failed");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // Shut down persistent Q&A sessions (kills CC child processes).
    qa_mgr.shutdown().await;

    mando_gateway::telemetry::shutdown_tracing();

    mando_gateway::instance::cleanup_files(args.dev);
    info!("mando-gw shutdown complete");
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
