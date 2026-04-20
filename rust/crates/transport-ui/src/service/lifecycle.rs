use std::time::Instant;

use crate::config::UPDATE_GRACE_PERIOD;
use crate::types::{UiDesiredState, UiStatus};

pub(crate) fn ui_status(
    desired_state: UiDesiredState,
    current_pid: Option<i32>,
    launch_available: bool,
    running: bool,
    last_error: Option<String>,
    degraded: bool,
    restart_count: u32,
) -> UiStatus {
    UiStatus {
        desired_state,
        current_pid,
        launch_available,
        running,
        last_error,
        degraded,
        restart_count,
    }
}

pub(crate) fn update_grace_expired(updating_since: Option<Instant>, now: Instant) -> bool {
    updating_since
        .map(|started| now.duration_since(started) >= UPDATE_GRACE_PERIOD)
        .unwrap_or(true)
}

pub(crate) fn should_skip_shutdown(desired_state: UiDesiredState) -> bool {
    desired_state == UiDesiredState::Updating
}
