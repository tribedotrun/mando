use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::info;

use crate::AppState;

pub struct BootstrapOptions {
    pub port_override: Option<u16>,
    pub dev_mode: bool,
    pub sandbox_mode: bool,
    pub unsafe_start: bool,
    pub start_time: Instant,
}

#[derive(Clone, Copy)]
pub struct RuntimeStartOptions {
    pub start_ui_monitor: bool,
    pub start_telegram: bool,
}

pub async fn start_runtime_services(state: &AppState, options: RuntimeStartOptions) {
    if options.start_ui_monitor {
        state
            .ui_runtime
            .start_monitor(&state.task_tracker, state.cancellation_token.clone());
    }

    state.captain.start_background_loops();
    state.scout.resume_pending_items().await;
    state.terminal.start_auto_resume();

    if options.start_telegram {
        let config = state.settings.load_config();
        if let Err(err) = state.telegram_runtime.configure(&config).await {
            tracing::warn!(
                module = "telegram",
                error = %err,
                "failed to start embedded telegram runtime"
            );
        }
    } else {
        info!(
            module = "mando-gateway",
            "telegram disabled via startup options"
        );
    }
}

pub struct GatewayBootstrap {
    pub state: AppState,
    pub host: String,
}

pub async fn bootstrap_gateway(
    config: settings::config::Config,
    bus: Arc<global_bus::EventBus>,
    options: BootstrapOptions,
) -> anyhow::Result<GatewayBootstrap> {
    let host = config.gateway.dashboard.host.clone();
    let listen_port = options
        .port_override
        .unwrap_or(config.gateway.dashboard.port);
    let runtime_paths = captain::resolve_captain_runtime_paths(&config);
    captain::set_active_captain_runtime_paths(runtime_paths.clone());

    let db = global_db::Db::open(&runtime_paths.task_db_path).await?;
    let db = Arc::new(db);

    let task_store = captain::TaskStore::new(db.pool().clone());
    let task_store = Arc::new(RwLock::new(task_store));
    let settings = Arc::new(
        settings::SettingsRuntime::bootstrap(
            config.clone(),
            db.pool().clone(),
            workflow_mode_for(&options),
        )
        .await?,
    );

    crate::startup::startup_reconciliation(db.pool()).await;

    let runtime_config = settings.load_config();
    if let Err(err) = captain::reconcile_on_startup(&runtime_config, db.pool()).await {
        if options.unsafe_start {
            tracing::error!(
                module = "startup",
                error = %err,
                "reconciliation failed — continuing under unsafe_start"
            );
        } else {
            tracing::error!(
                module = "startup",
                error = %err,
                "reconciliation failed — refusing to start (set MANDO_UNSAFE_START=1 to override)"
            );
            return Err(err);
        }
    }

    let terminal_default_model = if options.sandbox_mode {
        "haiku"
    } else {
        "sonnet"
    };
    let cc_state_dir = global_infra::paths::state_dir()
        .join("ops_sessions")
        .join("cc");
    let sessions_runtime = crate::session_backend::build_sessions_runtime(
        cc_state_dir,
        terminal_default_model,
        db.pool().clone(),
    );
    let cc_recovered = sessions_runtime.recover();
    if cc_recovered.recovered > 0 || cc_recovered.corrupt > 0 {
        info!(
            module = "mando-gateway",
            recovered = cc_recovered.recovered,
            corrupt = cc_recovered.corrupt,
            "recovered sessions from disk"
        );
    }

    let scout_workflow = settings.load_scout_workflow();
    let qa_session_mgr = scout::session_manager_from_workflow(&scout_workflow);
    let auth_token = transport_http::ensure_auth_token();
    let task_tracker = TaskTracker::new();
    let cancellation_token = CancellationToken::new();
    let terminal_host = Arc::new(terminal::TerminalHost::new(global_infra::paths::data_dir()));
    let scout_processing_semaphore = Arc::new(tokio::sync::Semaphore::new(4));
    let auto_title_notify = Arc::new(tokio::sync::Notify::new());
    let telegram_runtime = Arc::new(transport_tg::TelegramRuntime::new(listen_port, auth_token));
    let ui_runtime = Arc::new(transport_ui::UiRuntime::new(
        global_infra::paths::state_dir().join("ui-state.json"),
    ));
    let cleanup_sessions = {
        let sessions_runtime = sessions_runtime.clone();
        Arc::new(move || sessions_runtime.cleanup_expired()) as Arc<dyn Fn() -> usize + Send + Sync>
    };
    let captain_runtime = Arc::new(captain::CaptainRuntime::new(captain::CaptainRuntimeDeps {
        settings: settings.clone(),
        bus: bus.clone(),
        task_store: task_store.clone(),
        pool: db.pool().clone(),
        task_tracker: task_tracker.clone(),
        cancellation_token: cancellation_token.clone(),
        auto_title_notify,
        cleanup_expired_sessions: cleanup_sessions,
    }));
    let scout_runtime = Arc::new(scout::ScoutRuntime::new(
        settings.clone(),
        bus.clone(),
        db.pool().clone(),
        scout_processing_semaphore,
        task_tracker.clone(),
        cancellation_token.clone(),
        qa_session_mgr.clone(),
    ));
    let terminal_runtime = Arc::new(terminal::TerminalRuntime::new(
        terminal_host,
        settings.clone(),
        listen_port,
        task_tracker.clone(),
        cancellation_token.clone(),
        Arc::new(transport_http::ensure_auth_token),
    ));

    let state = AppState {
        settings,
        runtime_paths,
        bus,
        captain: captain_runtime,
        scout: scout_runtime,
        sessions: sessions_runtime,
        terminal: terminal_runtime,
        start_time: options.start_time,
        listen_port,
        task_tracker,
        cancellation_token,
        telegram_runtime,
        ui_runtime,
    };

    Ok(GatewayBootstrap { state, host })
}

fn workflow_mode_for(options: &BootstrapOptions) -> settings::WorkflowRuntimeMode {
    if options.dev_mode {
        settings::WorkflowRuntimeMode::Dev
    } else if options.sandbox_mode {
        settings::WorkflowRuntimeMode::Sandbox
    } else {
        settings::WorkflowRuntimeMode::Normal
    }
}
