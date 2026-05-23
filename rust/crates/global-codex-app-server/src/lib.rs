mod manager;
mod process;
mod request_params;
mod routing;
mod types;

pub use manager::{shared_manager, CodexAppServerManager};
pub use types::{AppServerEvent, CodexTurnConfig, StartTurnRequest, StartedTurn, StderrTail};

impl Default for CodexAppServerManager {
    fn default() -> Self {
        Self::new()
    }
}
