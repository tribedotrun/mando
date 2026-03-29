//! Internal row types for task queries — kept separate to respect file length limits.

use mando_types::session_ids::SessionIds;
use mando_types::task::{ItemStatus, Task, TaskRouting};

/// sqlx row type for the full task table.
#[derive(sqlx::FromRow)]
pub(super) struct TaskRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub project: Option<String>,
    pub worker: Option<String>,
    pub linear_id: Option<String>,
    pub resource: Option<String>,
    pub context: Option<String>,
    pub original_prompt: Option<String>,
    pub created_at: Option<String>,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub worker_started_at: Option<String>,
    pub intervention_count: i64,
    pub captain_review_trigger: Option<String>,
    pub session_ids: String,
    pub clarifier_questions: Option<String>,
    pub last_activity_at: Option<String>,
    pub plan: Option<String>,
    pub no_pr: i64,
    pub worker_seq: i64,
    pub reopen_seq: i64,
    pub reopen_source: Option<String>,
    pub images: Option<String>,
    pub retry_count: i64,
    pub escalation_report: Option<String>,
    pub source: Option<String>,
    pub archived_at: Option<String>,
}

impl TaskRow {
    pub fn into_task(self) -> Task {
        let status: ItemStatus = self.status.parse().unwrap_or_else(|_| {
            tracing::warn!(module = "task-db", status = %self.status, "unknown status");
            ItemStatus::New
        });
        let captain_review_trigger = self.captain_review_trigger.and_then(|s| {
            s.parse()
                .map_err(|_| {
                    tracing::warn!(module = "task-db", trigger = %s, "unknown trigger");
                })
                .ok()
        });
        Task {
            id: self.id,
            title: self.title,
            status,
            project: self.project,
            worker: self.worker,
            linear_id: self.linear_id,
            resource: self.resource,
            context: self.context,
            original_prompt: self.original_prompt,
            created_at: self.created_at,
            worktree: self.worktree,
            branch: self.branch,
            pr: self.pr,
            worker_started_at: self.worker_started_at,
            intervention_count: self.intervention_count,
            captain_review_trigger,
            session_ids: SessionIds::from_json(&self.session_ids),
            clarifier_questions: self.clarifier_questions,
            last_activity_at: self.last_activity_at,
            plan: self.plan,
            no_pr: self.no_pr != 0,
            worker_seq: self.worker_seq,
            reopen_seq: self.reopen_seq,
            reopen_source: self.reopen_source,
            images: self.images,
            retry_count: self.retry_count,
            escalation_report: self.escalation_report,
            source: self.source,
            archived_at: self.archived_at,
            rebase_worker: None,
            rebase_retries: 0,
            rebase_head_sha: None,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct RoutingRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub project: Option<String>,
    pub worker: Option<String>,
    pub linear_id: Option<String>,
    pub resource: Option<String>,
}

impl RoutingRow {
    pub fn into_routing(self) -> TaskRouting {
        let status: ItemStatus = self.status.parse().unwrap_or_else(|_| {
            tracing::warn!(module = "task-db", status = %self.status, "unknown status");
            ItemStatus::New
        });
        TaskRouting {
            id: self.id,
            title: self.title,
            status,
            project: self.project,
            worker: self.worker,
            linear_id: self.linear_id,
            resource: self.resource,
        }
    }
}
