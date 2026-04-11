//! mando-gateway — axum HTTP server for the Mando dashboard.
//!
//! Transport layer: thin handlers that parse requests, call domain
//! functions, and format JSON responses.

pub mod auth;
pub mod background_tasks;
pub mod config_manager;
pub mod hooks;
pub mod instance;
pub mod middleware;
pub(crate) mod response;
mod routes_ai;
mod routes_captain;
mod routes_captain_adopt;
mod routes_channels;
mod routes_clarifier;
mod routes_client_logs;
mod routes_config;
mod routes_ops;
mod routes_projects;
mod routes_scout;
mod routes_scout_ai;
mod routes_scout_bulk;
mod routes_scout_telegraph;
mod routes_sessions;
mod routes_task_actions;
mod routes_task_ask;
mod routes_task_detail;
mod routes_tasks;
mod routes_terminal;
mod routes_ui;
mod routes_workbenches;
mod routes_worktrees;
mod scout_notify;
pub mod server;
mod sse;
mod static_files;
pub mod telegram_runtime;
pub mod telemetry;
pub mod ui_runtime;
use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<mando_config::Config>>,
    pub config_manager: config_manager::ConfigManager,
    pub runtime_paths: mando_config::CaptainRuntimePaths,
    pub captain_workflow: Arc<ArcSwap<mando_config::CaptainWorkflow>>,
    pub scout_workflow: Arc<ArcSwap<mando_config::ScoutWorkflow>>,
    /// Serializes config/workflow write operations (read-modify-write).
    /// ArcSwap provides lock-free reads but doesn't serialize writers —
    /// concurrent config saves need this mutex to prevent lost updates.
    pub config_write_mu: Arc<Mutex<()>>,
    pub bus: Arc<mando_shared::EventBus>,
    /// `CcSessionManager` uses interior mutability (sync Mutex over the
    /// sessions HashMap) so all methods take `&self`. Holding an `Arc` here
    /// instead of `Arc<RwLock<_>>` lets concurrent `/api/ops/*` and
    /// `/api/tasks/ask` requests run in parallel without blocking on a
    /// write lock during the 10s+ CC API call.
    pub cc_session_mgr: Arc<mando_captain::io::cc_session::CcSessionManager>,
    pub task_store: Arc<RwLock<mando_captain::io::task_store::TaskStore>>,
    pub db: Arc<mando_db::Db>,
    pub qa_session_mgr: Arc<mando_scout::runtime::qa::QaSessionManager>,
    pub terminal_host: Arc<mando_terminal::TerminalHost>,
    pub start_time: Instant,
    pub listen_port: u16,
    /// When true, all CC invocations use sonnet instead of the configured model.
    pub dev_mode: bool,
    /// Tracks all fire-and-forget spawns so the process can await their
    /// completion on shutdown. Call `task_tracker.close()` then `.wait().await`
    /// in the shutdown path.
    pub task_tracker: TaskTracker,
    /// Cancellation signal for cooperative shutdown. Long-running loops
    /// (auto-tick, SSE streams) should check this via `tokio::select!`.
    pub cancellation_token: CancellationToken,
    pub telegram_runtime: Arc<telegram_runtime::TelegramRuntime>,
    pub ui_runtime: Arc<ui_runtime::UiRuntime>,
}

/// Force all workflow models to sonnet (dev mode cost savings).
pub fn apply_dev_model_overrides(
    captain_wf: &mut mando_config::CaptainWorkflow,
    scout_wf: &mut mando_config::ScoutWorkflow,
) {
    const DEV_MODEL: &str = "sonnet";
    captain_wf.models.worker = DEV_MODEL.into();
    captain_wf.models.captain = DEV_MODEL.into();
    captain_wf.models.clarifier = DEV_MODEL.into();
    captain_wf.models.todo_parse = DEV_MODEL.into();
    captain_wf.models.fallback = None;
    for model in scout_wf.models.values_mut() {
        *model = DEV_MODEL.into();
    }
    tracing::info!("dev mode: all models forced to {DEV_MODEL}");
}

/// Resolve a project display-name to its `github_repo` slug from config.
pub(crate) use mando_config::resolve_github_repo;

/// If the captain has exactly one project configured, return its
/// `github_repo` slug. Otherwise `None`. Both the gateway and tick.rs need
/// this to pick a default repo when no project name is supplied.
pub fn single_project_repo(captain: &mando_config::settings::CaptainConfig) -> Option<String> {
    if captain.projects.len() == 1 {
        captain
            .projects
            .values()
            .next()
            .and_then(|pc| pc.github_repo.clone())
    } else {
        None
    }
}

pub(crate) fn captain_notifier(
    state: &AppState,
    config: &mando_config::Config,
) -> mando_captain::runtime::notify::Notifier {
    let default_slug = single_project_repo(&config.captain);

    mando_captain::runtime::notify::Notifier::new(state.bus.clone())
        .with_repo_slug(default_slug)
        .with_notifications_enabled(true)
}

pub use server::start_server;
