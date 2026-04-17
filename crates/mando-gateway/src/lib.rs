//! mando-gateway — axum HTTP server for the Mando dashboard.
//!
//! Transport layer: thin handlers that parse requests, call domain
//! functions, and format JSON responses.

pub mod auto_title;
pub mod background_tasks;
pub mod config_manager;
pub mod credentials;
mod credentials_oauth;
pub mod hooks;
mod image_upload;
mod image_upload_ext;
pub mod instance;
pub mod response;
mod routes_ai;
mod routes_artifacts;
mod routes_captain;
mod routes_captain_adopt;
mod routes_channels;
mod routes_clarifier;
mod routes_client_logs;
mod routes_config;
mod routes_credentials;
mod routes_ops;
mod routes_projects;
pub mod routes_scout;
mod routes_scout_ai;
mod routes_scout_bulk;
mod routes_scout_telegraph;
mod routes_sessions;
mod routes_stats;
mod routes_task_actions;
mod routes_task_advisor;
mod routes_task_advisor_helpers;
mod routes_task_ask;
mod routes_task_detail;
mod routes_task_router;
mod routes_tasks;
mod routes_terminal;
mod routes_ui;
mod routes_workbenches;
mod routes_worktrees;
mod scout_notify;
pub mod server;
pub mod shutdown;
mod sse;
pub mod startup;
pub mod telegram_runtime;
pub mod telemetry;
pub mod ui_runtime;
use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio::sync::{Mutex, Notify, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<settings::config::Config>>,
    pub config_manager: config_manager::ConfigManager,
    pub runtime_paths: captain::config::CaptainRuntimePaths,
    pub captain_workflow: Arc<ArcSwap<settings::config::CaptainWorkflow>>,
    pub scout_workflow: Arc<ArcSwap<settings::config::ScoutWorkflow>>,
    /// Serializes config/workflow write operations (read-modify-write).
    /// ArcSwap provides lock-free reads but doesn't serialize writers —
    /// concurrent config saves need this mutex to prevent lost updates.
    pub config_write_mu: Arc<Mutex<()>>,
    pub bus: Arc<global_bus::EventBus>,
    /// `CcSessionManager` uses interior mutability (sync Mutex over the
    /// sessions HashMap) so all methods take `&self`. Holding an `Arc` here
    /// instead of `Arc<RwLock<_>>` lets concurrent `/api/ops/*` and
    /// `/api/tasks/ask` requests run in parallel without blocking on a
    /// write lock during the 10s+ CC API call.
    pub cc_session_mgr: Arc<captain::io::cc_session::CcSessionManager>,
    pub task_store: Arc<RwLock<captain::io::task_store::TaskStore>>,
    pub db: Arc<global_db::Db>,
    pub qa_session_mgr: Arc<scout::runtime::qa::QaSessionManager>,
    pub terminal_host: Arc<terminal::TerminalHost>,
    pub start_time: Instant,
    pub listen_port: u16,
    /// When true, all CC invocations use sonnet instead of the configured model.
    pub dev_mode: bool,
    /// When true, all CC invocations use haiku. Used by the sandbox for fast,
    /// cheap end-to-end tests. Mutually exclusive with `dev_mode`.
    pub sandbox_mode: bool,
    /// Tracks all fire-and-forget spawns so the process can await their
    /// completion on shutdown. Call `task_tracker.close()` then `.wait().await`
    /// in the shutdown path.
    pub task_tracker: TaskTracker,
    /// Cancellation signal for cooperative shutdown. Long-running loops
    /// (auto-tick, SSE streams) should check this via `tokio::select!`.
    pub cancellation_token: CancellationToken,
    pub telegram_runtime: Arc<telegram_runtime::TelegramRuntime>,
    pub ui_runtime: Arc<ui_runtime::UiRuntime>,
    /// Limits concurrent scout processing sessions (research, manual add, bulk).
    pub scout_processing_semaphore: Arc<Semaphore>,
    pub credential_mgr: Arc<credentials::CredentialManager>,
    /// Wakes the auto-title loop when a user submits their first prompt
    /// in a terminal workbench, so it doesn't have to wait for the next
    /// poll cycle.
    pub auto_title_notify: Arc<Notify>,
}

/// Force all workflow models to sonnet (dev mode cost savings).
pub fn apply_dev_model_overrides(
    captain_wf: &mut settings::config::CaptainWorkflow,
    scout_wf: &mut settings::config::ScoutWorkflow,
) {
    apply_model_overrides(captain_wf, scout_wf, "sonnet", "dev");
}

/// Force all workflow models to haiku (sandbox: fast, cheap end-to-end tests).
pub fn apply_sandbox_model_overrides(
    captain_wf: &mut settings::config::CaptainWorkflow,
    scout_wf: &mut settings::config::ScoutWorkflow,
) {
    apply_model_overrides(captain_wf, scout_wf, "haiku", "sandbox");
}

fn apply_model_overrides(
    captain_wf: &mut settings::config::CaptainWorkflow,
    scout_wf: &mut settings::config::ScoutWorkflow,
    model: &str,
    mode_label: &str,
) {
    captain_wf.models.worker = model.into();
    captain_wf.models.captain = model.into();
    captain_wf.models.clarifier = model.into();
    captain_wf.models.todo_parse = model.into();
    for m in scout_wf.models.values_mut() {
        *m = model.into();
    }
    tracing::info!("{mode_label} mode: all models forced to {model}");
}

/// Resolve a project display-name to its `github_repo` slug from config.
pub use settings::config::resolve_github_repo;

/// If the captain has exactly one project configured, return its
/// `github_repo` slug. Otherwise `None`. Both the gateway and tick.rs need
/// this to pick a default repo when no project name is supplied.
pub fn single_project_repo(captain: &settings::config::settings::CaptainConfig) -> Option<String> {
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

pub fn captain_notifier(
    state: &AppState,
    config: &settings::config::Config,
) -> captain::runtime::notify::Notifier {
    let default_slug = single_project_repo(&config.captain);

    captain::runtime::notify::Notifier::new(state.bus.clone())
        .with_repo_slug(default_slug)
        .with_notifications_enabled(true)
}

pub use server::start_server;
