use anyhow::{bail, Result};

use crate::{ResearchRunStatus, ScoutError, ScoutStatus};

fn invalid_transition(command: &'static str, status: ScoutStatus) -> anyhow::Error {
    ScoutError::InvalidTransition {
        command,
        status: status.as_str(),
    }
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutItemCommand {
    MarkPending,
    MarkFetched,
    MarkProcessed,
    MarkError,
    Save,
    Archive,
}

impl ScoutItemCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MarkPending => "mark_pending",
            Self::MarkFetched => "mark_fetched",
            Self::MarkProcessed => "mark_processed",
            Self::MarkError => "mark_error",
            Self::Save => "save",
            Self::Archive => "archive",
        }
    }
}

pub fn apply_item_command(current: ScoutStatus, command: ScoutItemCommand) -> Result<ScoutStatus> {
    let next = match command {
        ScoutItemCommand::MarkPending => match current {
            ScoutStatus::Pending => ScoutStatus::Pending,
            ScoutStatus::Fetched
            | ScoutStatus::Processed
            | ScoutStatus::Saved
            | ScoutStatus::Archived
            | ScoutStatus::Error => ScoutStatus::Pending,
        },
        ScoutItemCommand::MarkFetched => match current {
            ScoutStatus::Pending | ScoutStatus::Error => ScoutStatus::Fetched,
            other => return Err(invalid_transition(command.as_str(), other)),
        },
        ScoutItemCommand::MarkProcessed => match current {
            ScoutStatus::Pending | ScoutStatus::Fetched | ScoutStatus::Error => {
                ScoutStatus::Processed
            }
            other => return Err(invalid_transition(command.as_str(), other)),
        },
        ScoutItemCommand::MarkError => match current {
            ScoutStatus::Pending
            | ScoutStatus::Fetched
            | ScoutStatus::Processed
            | ScoutStatus::Saved
            | ScoutStatus::Error => ScoutStatus::Error,
            ScoutStatus::Archived => return Err(invalid_transition(command.as_str(), current)),
        },
        ScoutItemCommand::Save => match current {
            ScoutStatus::Processed | ScoutStatus::Archived => ScoutStatus::Saved,
            other => return Err(invalid_transition(command.as_str(), other)),
        },
        ScoutItemCommand::Archive => match current {
            ScoutStatus::Pending
            | ScoutStatus::Fetched
            | ScoutStatus::Processed
            | ScoutStatus::Saved
            | ScoutStatus::Error
            | ScoutStatus::Archived => ScoutStatus::Archived,
        },
    };
    Ok(next)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchRunCommand {
    Start,
    Complete,
    Fail,
    RecoverInterrupted,
}

impl ResearchRunCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Complete => "complete",
            Self::Fail => "fail",
            Self::RecoverInterrupted => "recover_interrupted",
        }
    }
}

pub fn apply_research_command(
    current: Option<ResearchRunStatus>,
    command: ResearchRunCommand,
) -> Result<ResearchRunStatus> {
    let next = match (current, command) {
        (None, ResearchRunCommand::Start) => ResearchRunStatus::Running,
        (Some(ResearchRunStatus::Running), ResearchRunCommand::Complete) => ResearchRunStatus::Done,
        (Some(ResearchRunStatus::Running), ResearchRunCommand::Fail)
        | (Some(ResearchRunStatus::Running), ResearchRunCommand::RecoverInterrupted) => {
            ResearchRunStatus::Failed
        }
        (state, cmd) => bail!(
            "cannot apply research command {} from {:?}",
            cmd.as_str(),
            state
        ),
    };
    Ok(next)
}
