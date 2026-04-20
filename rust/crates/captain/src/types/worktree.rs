use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeEntry {
    pub project: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWorktree {
    pub path: String,
    pub branch: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workbench_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum CreateWorktreeOutcome {
    Created(CreatedWorktree),
    ProjectNotFound(String),
    Conflict(String),
}

#[derive(Debug, Clone)]
pub enum RemoveWorktreeOutcome {
    Removed,
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreePruneError {
    pub project: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanupWorktreesReport {
    pub orphans: Vec<String>,
    pub removed: Vec<String>,
    pub prune_errors: Vec<WorktreePruneError>,
}
