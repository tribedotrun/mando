//! Task and ItemStatus — the core task domain types.

use serde::{Deserialize, Serialize};

use super::session_ids::SessionIds;
pub use super::task_status::{
    ItemStatus, ReviewTrigger, ACTIONABLE_TERMINAL, ALL_STATUSES, FINALIZED, REOPENABLE, REWORKABLE,
};
pub use super::task_update::{TaskUpdateError, UpdateTaskInput};

/// Routing fields — lightweight struct for captain tick hot path.
/// Populated from tasks WHERE the owning workbench is not archived.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskRouting {
    pub id: i64,
    pub title: String,
    pub status: ItemStatus,
    pub project_id: i64,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

impl Default for TaskRouting {
    fn default() -> Self {
        Self {
            id: 0,
            title: String::new(),
            status: ItemStatus::New,
            project_id: 0,
            project: String::new(),
            worker: None,
            resource: None,
        }
    }
}

/// A task — the fundamental unit of work in Mando.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: i64,
    pub title: String,
    /// Intentionally crate-private. Writes go through
    /// `service::lifecycle::apply_transition` (or the test-only
    /// `set_status_for_tests` shim). External crates read via the
    /// [`Task::status`] getter. This lets the compiler reject any
    /// external `task.status = ...` pattern; the
    /// `check_status_mutation_surface.py` scanner catches the same
    /// pattern inside `captain`.
    pub(crate) status: ItemStatus,
    /// DB column: `project_id INTEGER NOT NULL REFERENCES projects(id)`.
    pub project_id: i64,
    /// Project name -- populated via JOIN on projects table, not a DB column.
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub workbench_id: i64,
    /// Worktree path -- not a DB column on tasks; populated via JOIN on
    /// workbenches.  Kept on the struct so existing read-sites work unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// PR number (integer). Stored as `pr_number INTEGER` in DB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_started_at: Option<String>,
    pub intervention_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captain_review_trigger: Option<ReviewTrigger>,
    pub session_ids: SessionIds,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub no_pr: bool,
    pub no_auto_merge: bool,
    pub planning: bool,
    pub worker_seq: i64,
    pub reopen_seq: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopened_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopen_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<String>,
    pub review_fail_count: i64,
    pub clarifier_fail_count: i64,
    pub spawn_fail_count: i64,
    pub merge_fail_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation_report: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub rev: i64,
    /// GitHub repo slug -- populated via JOIN on projects table, not a DB column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    #[serde(skip)]
    pub rebase_worker: Option<String>,
    #[serde(skip)]
    pub rebase_retries: i64,
    #[serde(skip)]
    pub rebase_head_sha: Option<String>,
}

impl Default for Task {
    fn default() -> Self {
        Self::new("")
    }
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
    /// Read-only accessor for `status`. External crates use this because
    /// the field itself is crate-private (see the field doc for rationale).
    pub fn status(&self) -> ItemStatus {
        self.status
    }

    /// Test-only initializer. Bypasses the lifecycle transition table so
    /// fixtures can land a task in an arbitrary status without wiring up
    /// the full state machine. Every other write path must go through
    /// `service::lifecycle::apply_transition`.
    ///
    /// Gated by `cfg(any(test, feature = "testing"))` so production code
    /// paths in this crate and every downstream crate are compiled
    /// without the shim, preserving the type-level "status is read-only
    /// publicly" guarantee.
    #[cfg(any(test, feature = "testing"))]
    #[doc(hidden)]
    pub fn set_status_for_tests(&mut self, status: ItemStatus) {
        self.status = status;
    }

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
            workbench_id: 0,
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
            no_auto_merge: false,
            planning: false,
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

    /// Apply a typed patch to this task.
    ///
    /// Only fields set to `Some(…)` in `input` are modified.
    /// For nullable fields an inner `Some(None)` clears the field; `Some(Some(v))` sets it.
    pub fn apply_update(&mut self, input: UpdateTaskInput) {
        if let Some(v) = input.title {
            self.title = v;
        }
        if let Some(v) = input.project_id {
            self.project_id = v;
        }
        if let Some(v) = input.worker {
            self.worker = v;
        }
        if let Some(v) = input.resource {
            self.resource = v;
        }
        if let Some(v) = input.context {
            self.context = v;
        }
        if let Some(v) = input.original_prompt {
            self.original_prompt = v;
        }
        if let Some(v) = input.created_at {
            self.created_at = v;
        }
        if let Some(v) = input.workbench_id {
            self.workbench_id = v;
        }
        if let Some(v) = input.pr_number {
            self.pr_number = v;
        }
        if let Some(v) = input.worker_started_at {
            self.worker_started_at = v;
        }
        if let Some(v) = input.intervention_count {
            self.intervention_count = v;
        }
        if let Some(v) = input.captain_review_trigger {
            self.captain_review_trigger = v;
        }
        if let Some(v) = input.last_activity_at {
            self.last_activity_at = v;
        }
        if let Some(v) = input.plan {
            self.plan = v;
        }
        if let Some(v) = input.no_pr {
            self.no_pr = v;
        }
        if let Some(v) = input.no_auto_merge {
            self.no_auto_merge = v;
        }
        if let Some(v) = input.worker_seq {
            self.worker_seq = v;
        }
        if let Some(v) = input.reopen_seq {
            self.reopen_seq = v;
        }
        if let Some(v) = input.reopened_at {
            self.reopened_at = v;
        }
        if let Some(v) = input.reopen_source {
            self.reopen_source = v;
        }
        if let Some(v) = input.images {
            self.images = v;
        }
        if let Some(v) = input.review_fail_count {
            self.review_fail_count = v;
        }
        if let Some(v) = input.clarifier_fail_count {
            self.clarifier_fail_count = v;
        }
        if let Some(v) = input.spawn_fail_count {
            self.spawn_fail_count = v;
        }
        if let Some(v) = input.merge_fail_count {
            self.merge_fail_count = v;
        }
        if let Some(v) = input.escalation_report {
            self.escalation_report = v;
        }
        if let Some(v) = input.source {
            self.source = v;
        }
        if let Some(v) = input.session_ids {
            self.session_ids = v;
        }
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

    #[test]
    fn apply_update_sets_title() {
        let mut task = Task::new("original");
        task.apply_update(UpdateTaskInput {
            title: Some("updated".into()),
            ..Default::default()
        });
        assert_eq!(task.title, "updated");
    }

    #[test]
    fn apply_update_clears_nullable_field() {
        let mut task = Task::new("test");
        task.worker = Some("worker-1".into());
        task.apply_update(UpdateTaskInput {
            worker: Some(None),
            ..Default::default()
        });
        assert!(task.worker.is_none());
    }

    #[test]
    fn apply_update_leaves_untouched_fields_alone() {
        let mut task = Task::new("test");
        task.context = Some("ctx".into());
        task.apply_update(UpdateTaskInput {
            title: Some("new".into()),
            ..Default::default()
        });
        assert_eq!(task.context.as_deref(), Some("ctx"));
    }
}
