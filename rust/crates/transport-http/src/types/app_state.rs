use std::sync::Arc;
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<settings::SettingsRuntime>,
    pub runtime_paths: captain::CaptainRuntimePaths,
    pub bus: Arc<global_bus::EventBus>,
    pub captain: Arc<captain::CaptainRuntime>,
    pub scout: Arc<scout::ScoutRuntime>,
    pub sessions: Arc<sessions::SessionsRuntime>,
    pub terminal: Arc<terminal::TerminalRuntime>,
    pub start_time: Instant,
    pub listen_port: u16,
    pub task_tracker: TaskTracker,
    pub cancellation_token: CancellationToken,
    pub telegram_runtime: Arc<transport_tg::TelegramRuntime>,
    pub ui_runtime: Arc<transport_ui::UiRuntime>,
}
