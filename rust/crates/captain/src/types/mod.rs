pub mod artifact;
pub mod ask_history;
pub mod captain;
pub mod effect_request;
pub mod git_error;
pub mod pid;
pub mod rebase_state;
pub mod session_ids;
pub mod task;
pub mod task_action;
pub mod task_create;
pub mod task_status;
pub mod task_update;
pub mod timeline;
pub mod workbench;
pub mod workbench_layout;
pub mod worktree;

pub use artifact::{
    ArtifactMedia, ArtifactType, EvidenceArtifactCreated, EvidenceFileSpec, TaskArtifact,
    UpdateArtifactMediaOutcome,
};
pub use ask_history::AskHistoryEntry;
pub use captain::{Action, ActionKind, TickMode, TickResult, WorkerContext};
pub use effect_request::EffectRequest;
pub use git_error::{find_git_error, GitError};
pub use pid::Pid;
pub use rebase_state::{RebaseState, RebaseStatus};
pub use session_ids::SessionIds;
pub use task::{
    parse_pr_number, pr_label, pr_url, ItemStatus, ReviewTrigger, Task, TaskRouting,
    TaskUpdateError, UpdateTaskInput, ACTIONABLE_TERMINAL, ALL_REVIEW_TRIGGERS, ALL_STATUSES,
    FINALIZED, REOPENABLE, REWORKABLE,
};
pub use task_action::{find_task_action_error, TaskActionError};
pub use task_create::{find_task_create_error, TaskCreateError};
pub use timeline::{TimelineEvent, TimelineEventPayload};
pub use workbench::{workbench_title_now, Workbench, WorkbenchPatch, WorkbenchPatchOutcome};
pub use workbench_layout::{PanelState, WorkbenchLayout};
pub use worktree::{
    CleanupWorktreesReport, CreateWorktreeOutcome, CreatedWorktree, RemoveWorktreeOutcome,
    WorktreeEntry, WorktreePruneError,
};

pub(crate) fn default_rev() -> i64 {
    1
}
