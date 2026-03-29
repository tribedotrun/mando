//! Task and ItemStatus — the core task domain types.

use serde::{Deserialize, Serialize};

pub use super::task_status::{
    ItemStatus, ReviewTrigger, ACTIONABLE_TERMINAL, ALL_STATUSES, FINALIZED, REOPENABLE, REWORKABLE,
};
pub use super::task_update::TaskUpdateError;
use super::task_update::{expect_boolish_field, expect_i64_field, expect_string_field};
use crate::SessionIds;

/// Routing fields — lightweight struct for captain tick hot path.
/// Populated from `SELECT id, title, status, project, worker, linear_id, resource
/// FROM tasks WHERE archived_at IS NULL`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRouting {
    pub id: i64,
    pub title: String,
    pub status: ItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// A task — the fundamental unit of work in Mando.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub status: ItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_started_at: Option<String>,
    #[serde(default)]
    pub intervention_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captain_review_trigger: Option<ReviewTrigger>,
    #[serde(default)]
    pub session_ids: SessionIds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarifier_questions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default)]
    pub no_pr: bool,
    #[serde(default)]
    pub worker_seq: i64,
    #[serde(default)]
    pub reopen_seq: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<String>,
    #[serde(default)]
    pub retry_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_report: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(skip)]
    pub rebase_worker: Option<String>,
    #[serde(skip)]
    pub rebase_retries: i64,
    #[serde(skip)]
    pub rebase_head_sha: Option<String>,
}

impl Task {
    /// Create a minimal task with just a title. ID is 0 (placeholder until INSERT).
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: 0,
            title: title.into(),
            status: ItemStatus::New,
            project: None,
            worker: None,
            linear_id: None,
            resource: None,
            context: None,
            original_prompt: None,
            created_at: None,
            worktree: None,
            branch: None,
            pr: None,
            worker_started_at: None,
            intervention_count: 0,
            captain_review_trigger: None,
            session_ids: SessionIds::default(),
            clarifier_questions: None,
            last_activity_at: None,
            plan: None,
            no_pr: false,
            worker_seq: 0,
            reopen_seq: 0,
            reopen_source: None,
            images: None,
            retry_count: 0,
            escalation_report: None,
            source: None,
            archived_at: None,
            rebase_worker: None,
            rebase_retries: 0,
            rebase_head_sha: None,
        }
    }

    /// Best identifier for logging: linear_id > numeric id.
    pub fn best_id(&self) -> String {
        self.linear_id
            .as_deref()
            .map(String::from)
            .unwrap_or_else(|| self.id.to_string())
    }

    /// Extract routing fields for the captain tick hot path.
    pub fn routing(&self) -> TaskRouting {
        TaskRouting {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            project: self.project.clone(),
            worker: self.worker.clone(),
            linear_id: self.linear_id.clone(),
            resource: self.resource.clone(),
        }
    }

    /// Set a field by name (for JSON-driven updates).
    pub fn set_field(
        &mut self,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), TaskUpdateError> {
        match key {
            "title" => self.title = expect_string_field(key, value)?.to_string(),
            "status" => {
                let raw = expect_string_field(key, value)?;
                let status: ItemStatus = raw
                    .parse()
                    .map_err(|_| TaskUpdateError::InvalidStatus(raw.to_string()))?;
                self.status = status;
                self.last_activity_at = Some(crate::now_rfc3339());
            }
            "project" => self.project = Some(expect_string_field(key, value)?.to_string()),
            "worker" => self.worker = Some(expect_string_field(key, value)?.to_string()),
            "linear_id" => self.linear_id = Some(expect_string_field(key, value)?.to_string()),
            "resource" => self.resource = Some(expect_string_field(key, value)?.to_string()),
            "context" => self.context = Some(expect_string_field(key, value)?.to_string()),
            "original_prompt" => {
                self.original_prompt = Some(expect_string_field(key, value)?.to_string())
            }
            "created_at" => self.created_at = Some(expect_string_field(key, value)?.to_string()),
            "worktree" => self.worktree = Some(expect_string_field(key, value)?.to_string()),
            "branch" => self.branch = Some(expect_string_field(key, value)?.to_string()),
            "pr" => self.pr = Some(expect_string_field(key, value)?.to_string()),
            "worker_started_at" => {
                self.worker_started_at = Some(expect_string_field(key, value)?.to_string())
            }
            "intervention_count" => self.intervention_count = expect_i64_field(key, value)?,
            "captain_review_trigger" => {
                let raw = expect_string_field(key, value)?;
                let trigger: ReviewTrigger =
                    raw.parse().map_err(|_| TaskUpdateError::InvalidFieldType {
                        field: key.into(),
                        expected: "review trigger",
                    })?;
                self.captain_review_trigger = Some(trigger);
            }
            "clarifier_questions" => {
                self.clarifier_questions = Some(expect_string_field(key, value)?.to_string())
            }
            "last_activity_at" => {
                self.last_activity_at = Some(expect_string_field(key, value)?.to_string())
            }
            "plan" => self.plan = Some(expect_string_field(key, value)?.to_string()),
            "no_pr" => self.no_pr = expect_boolish_field(key, value)?,
            "worker_seq" => self.worker_seq = expect_i64_field(key, value)?,
            "reopen_seq" => self.reopen_seq = expect_i64_field(key, value)?,
            "reopen_source" => {
                self.reopen_source = Some(expect_string_field(key, value)?.to_string())
            }
            "images" => self.images = Some(expect_string_field(key, value)?.to_string()),
            "retry_count" => self.retry_count = expect_i64_field(key, value)?,
            "escalation_report" => {
                self.escalation_report = Some(expect_string_field(key, value)?.to_string())
            }
            "source" => self.source = Some(expect_string_field(key, value)?.to_string()),
            _ => return Err(TaskUpdateError::UnknownField(key.into())),
        }
        Ok(())
    }

    /// Clear a field by name (set to None/default).
    pub fn clear_field(&mut self, key: &str) -> Result<(), TaskUpdateError> {
        match key {
            "project" => self.project = None,
            "worker" => self.worker = None,
            "linear_id" => self.linear_id = None,
            "resource" => self.resource = None,
            "context" => self.context = None,
            "original_prompt" => self.original_prompt = None,
            "created_at" => self.created_at = None,
            "worktree" => self.worktree = None,
            "branch" => self.branch = None,
            "pr" => self.pr = None,
            "worker_started_at" => self.worker_started_at = None,
            "intervention_count" => self.intervention_count = 0,
            "captain_review_trigger" => self.captain_review_trigger = None,
            "clarifier_questions" => self.clarifier_questions = None,
            "last_activity_at" => self.last_activity_at = None,
            "plan" => self.plan = None,
            "no_pr" => self.no_pr = false,
            "worker_seq" => self.worker_seq = 0,
            "reopen_seq" => self.reopen_seq = 0,
            "reopen_source" => self.reopen_source = None,
            "images" => self.images = None,
            "retry_count" => self.retry_count = 0,
            "escalation_report" => self.escalation_report = None,
            "source" => self.source = None,
            "title" | "status" => return Err(TaskUpdateError::FieldCannotBeNull(key.into())),
            _ => return Err(TaskUpdateError::UnknownField(key.into())),
        }
        Ok(())
    }
}
