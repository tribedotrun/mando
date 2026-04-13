//! Task and ItemStatus — the core task domain types.

use serde::{Deserialize, Serialize};

pub use super::task_status::{
    ItemStatus, ReviewTrigger, ACTIONABLE_TERMINAL, ALL_STATUSES, FINALIZED, REOPENABLE, REWORKABLE,
};
pub use super::task_update::TaskUpdateError;
use super::task_update::{expect_boolish_field, expect_i64_field, expect_string_field};
use crate::SessionIds;

/// Routing fields — lightweight struct for captain tick hot path.
/// Populated from tasks WHERE the owning workbench is not archived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRouting {
    pub id: i64,
    pub title: String,
    pub status: ItemStatus,
    pub project_id: i64,
    #[serde(default)]
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// A task — the fundamental unit of work in Mando.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub status: ItemStatus,
    /// DB column: `project_id INTEGER NOT NULL REFERENCES projects(id)`.
    pub project_id: i64,
    /// Project name -- populated via JOIN on projects table, not a DB column.
    #[serde(default)]
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbench_id: Option<i64>,
    /// Worktree path -- not a DB column on tasks; populated via JOIN on
    /// workbenches.  Kept on the struct so existing read-sites work unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// PR number (integer). Stored as `pr_number INTEGER` in DB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_started_at: Option<String>,
    #[serde(default)]
    pub intervention_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captain_review_trigger: Option<ReviewTrigger>,
    #[serde(default)]
    pub session_ids: SessionIds,
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
    pub reopened_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<String>,
    #[serde(default)]
    pub review_fail_count: i64,
    #[serde(default)]
    pub clarifier_fail_count: i64,
    #[serde(default)]
    pub spawn_fail_count: i64,
    #[serde(default)]
    pub merge_fail_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_report: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default = "crate::default_rev")]
    pub rev: i64,
    /// GitHub repo slug -- populated via JOIN on projects table, not a DB column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    #[serde(skip)]
    pub rebase_worker: Option<String>,
    #[serde(skip)]
    pub rebase_retries: i64,
    #[serde(skip)]
    pub rebase_head_sha: Option<String>,
}

/// Parse a PR reference from any format (full URL, `#N`, bare `N`) into an integer.
pub fn parse_pr_number(pr: &str) -> Option<i64> {
    // Full URL: …/pull/123 or …/pull/123/files
    if let Some(idx) = pr.rfind("/pull/") {
        let after = &pr[idx + 6..];
        let num_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        return after[..num_end].parse().ok();
    }
    // #N or bare N
    pr.trim_start_matches('#').parse().ok()
}

/// Format a PR number as a short label: `#123`.
pub fn pr_label(pr_number: i64) -> String {
    format!("#{pr_number}")
}

/// Build a full GitHub PR URL from a repo slug and PR number.
pub fn pr_url(github_repo: &str, pr_number: i64) -> String {
    format!("https://github.com/{github_repo}/pull/{pr_number}")
}

impl Task {
    /// Create a minimal task with just a title. ID and project_id are 0
    /// (placeholders until INSERT / project resolution).
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: 0,
            title: title.into(),
            status: ItemStatus::New,
            project_id: 0,
            project: String::new(),
            worker: None,
            resource: None,
            context: None,
            original_prompt: None,
            created_at: None,
            workbench_id: None,
            worktree: None,
            branch: None,
            pr_number: None,
            worker_started_at: None,
            intervention_count: 0,
            captain_review_trigger: None,
            session_ids: SessionIds::default(),
            last_activity_at: None,
            plan: None,
            no_pr: false,
            worker_seq: 0,
            reopen_seq: 0,
            reopened_at: None,
            reopen_source: None,
            images: None,
            review_fail_count: 0,
            clarifier_fail_count: 0,
            spawn_fail_count: 0,
            merge_fail_count: 0,
            escalation_report: None,
            source: None,
            rev: 1,
            github_repo: None,
            rebase_worker: None,
            rebase_retries: 0,
            rebase_head_sha: None,
        }
    }

    /// Extract routing fields for the captain tick hot path.
    #[must_use]
    pub fn routing(&self) -> TaskRouting {
        TaskRouting {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            project_id: self.project_id,
            project: self.project.clone(),
            worker: self.worker.clone(),
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
            "project_id" => self.project_id = expect_i64_field(key, value)?,
            "worker" => self.worker = Some(expect_string_field(key, value)?.to_string()),
            "resource" => self.resource = Some(expect_string_field(key, value)?.to_string()),
            "context" => self.context = Some(expect_string_field(key, value)?.to_string()),
            "original_prompt" => {
                self.original_prompt = Some(expect_string_field(key, value)?.to_string())
            }
            "created_at" => self.created_at = Some(expect_string_field(key, value)?.to_string()),
            "workbench_id" => self.workbench_id = Some(expect_i64_field(key, value)?),
            "pr_number" => self.pr_number = Some(expect_i64_field(key, value)?),
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
            "last_activity_at" => {
                self.last_activity_at = Some(expect_string_field(key, value)?.to_string())
            }
            "plan" => self.plan = Some(expect_string_field(key, value)?.to_string()),
            "no_pr" => self.no_pr = expect_boolish_field(key, value)?,
            "worker_seq" => self.worker_seq = expect_i64_field(key, value)?,
            "reopen_seq" => self.reopen_seq = expect_i64_field(key, value)?,
            "reopened_at" => self.reopened_at = Some(expect_string_field(key, value)?.to_string()),
            "reopen_source" => {
                self.reopen_source = Some(expect_string_field(key, value)?.to_string())
            }
            "images" => self.images = Some(expect_string_field(key, value)?.to_string()),
            "review_fail_count" => self.review_fail_count = expect_i64_field(key, value)?,
            "clarifier_fail_count" => self.clarifier_fail_count = expect_i64_field(key, value)?,
            "spawn_fail_count" => self.spawn_fail_count = expect_i64_field(key, value)?,
            "merge_fail_count" => self.merge_fail_count = expect_i64_field(key, value)?,
            "escalation_report" => {
                self.escalation_report = Some(expect_string_field(key, value)?.to_string())
            }
            "source" => self.source = Some(expect_string_field(key, value)?.to_string()),
            "session_ids" => {
                self.session_ids = serde_json::from_value(value.clone()).map_err(|_| {
                    TaskUpdateError::InvalidFieldType {
                        field: key.into(),
                        expected: "session_ids object",
                    }
                })?;
            }
            // Legacy fields -- no longer DB columns, accepted as no-ops for
            // backward compat with older PATCH payloads.
            "worktree" | "branch" | "pr" | "github_repo" => {}
            _ => return Err(TaskUpdateError::UnknownField(key.into())),
        }
        Ok(())
    }

    /// Clear a field by name (set to None/default).
    pub fn clear_field(&mut self, key: &str) -> Result<(), TaskUpdateError> {
        match key {
            "project" | "project_id" | "workbench_id" => {
                return Err(TaskUpdateError::FieldCannotBeNull(key.into()))
            }
            "worker" => self.worker = None,
            "resource" => self.resource = None,
            "context" => self.context = None,
            "original_prompt" => self.original_prompt = None,
            "created_at" => self.created_at = None,
            "pr_number" => self.pr_number = None,
            "worker_started_at" => self.worker_started_at = None,
            "intervention_count" => self.intervention_count = 0,
            "captain_review_trigger" => self.captain_review_trigger = None,
            "last_activity_at" => self.last_activity_at = None,
            "plan" => self.plan = None,
            "no_pr" => self.no_pr = false,
            "worker_seq" => self.worker_seq = 0,
            "reopen_seq" => self.reopen_seq = 0,
            "reopened_at" => self.reopened_at = None,
            "reopen_source" => self.reopen_source = None,
            "images" => self.images = None,
            "review_fail_count" => self.review_fail_count = 0,
            "clarifier_fail_count" => self.clarifier_fail_count = 0,
            "spawn_fail_count" => self.spawn_fail_count = 0,
            "merge_fail_count" => self.merge_fail_count = 0,
            "escalation_report" => self.escalation_report = None,
            "source" => self.source = None,
            "session_ids" => self.session_ids = SessionIds::default(),
            "title" | "status" => return Err(TaskUpdateError::FieldCannotBeNull(key.into())),
            // Legacy fields -- accepted as no-ops (see set_field).
            "worktree" | "branch" | "pr" | "github_repo" => {}
            _ => return Err(TaskUpdateError::UnknownField(key.into())),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_number_full_url() {
        assert_eq!(
            parse_pr_number("https://github.com/acme/widgets/pull/116"),
            Some(116)
        );
    }

    #[test]
    fn parse_pr_number_trailing_path() {
        assert_eq!(
            parse_pr_number("https://github.com/acme/widgets/pull/123/files"),
            Some(123)
        );
    }

    #[test]
    fn parse_pr_number_short_ref() {
        assert_eq!(parse_pr_number("#334"), Some(334));
    }

    #[test]
    fn parse_pr_number_bare_number() {
        assert_eq!(parse_pr_number("99"), Some(99));
    }

    #[test]
    fn parse_pr_number_invalid() {
        assert_eq!(parse_pr_number(""), None);
        assert_eq!(parse_pr_number("#"), None);
        assert_eq!(parse_pr_number("not-a-number"), None);
    }

    #[test]
    fn pr_label_format() {
        assert_eq!(pr_label(42), "#42");
    }

    #[test]
    fn pr_url_format() {
        assert_eq!(
            pr_url("acme/widgets", 42),
            "https://github.com/acme/widgets/pull/42"
        );
    }
}
