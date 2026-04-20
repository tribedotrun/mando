//! Session lifecycle state-machine helpers.

use anyhow::{bail, Result};
use global_types::SessionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLifecycleCommand {
    Start,
    Resume,
    Stop,
    Fail,
}

impl SessionLifecycleCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Fail => "fail",
        }
    }
}

pub fn infer_command(
    current: Option<SessionStatus>,
    next: SessionStatus,
    resumed: bool,
) -> Result<SessionLifecycleCommand> {
    let command = match (current, next, resumed) {
        (None, SessionStatus::Running, false) => SessionLifecycleCommand::Start,
        (None, SessionStatus::Running, true) => SessionLifecycleCommand::Resume,
        (None, SessionStatus::Stopped, _) => SessionLifecycleCommand::Stop,
        (None, SessionStatus::Failed, _) => SessionLifecycleCommand::Fail,
        (Some(SessionStatus::Running), SessionStatus::Stopped, _) => SessionLifecycleCommand::Stop,
        (Some(SessionStatus::Running), SessionStatus::Failed, _) => SessionLifecycleCommand::Fail,
        // PR #886: a previously-stopped session can be downgraded to
        // Failed when a subsequent resume turn surfaces a CC error. The
        // clarifier HTTP path re-uses the same session_id across resumes,
        // so the existing row must accept a Fail command to carry the
        // error/api_error_status columns back to the UI.
        (Some(SessionStatus::Stopped), SessionStatus::Failed, _) => SessionLifecycleCommand::Fail,
        (Some(SessionStatus::Stopped), SessionStatus::Running, false) => {
            SessionLifecycleCommand::Start
        }
        (Some(SessionStatus::Stopped), SessionStatus::Running, true) => {
            SessionLifecycleCommand::Resume
        }
        (Some(SessionStatus::Failed), SessionStatus::Running, false) => {
            SessionLifecycleCommand::Start
        }
        (Some(SessionStatus::Failed), SessionStatus::Running, true) => {
            SessionLifecycleCommand::Resume
        }
        (Some(current), next, _) if current == next => {
            bail!("session is already in {}", next.as_str())
        }
        (state, next, _) => bail!(
            "cannot transition session from {:?} to {}",
            state,
            next.as_str()
        ),
    };
    Ok(command)
}
