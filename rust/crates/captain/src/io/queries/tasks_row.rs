//! Internal row types for task queries — kept separate to respect file length limits.

use crate::{ItemStatus, SessionIds, Task, TaskRouting};
use anyhow::{Context, Result};

/// sqlx row type for the full task table.
/// SELECT includes JOINed columns: project (from projects.name),
/// worktree (from workbenches), github_repo (from projects).
#[derive(sqlx::FromRow)]
pub(super) struct TaskRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub project_id: i64,
    /// JOINed from projects.name
    pub project: String,
    pub worker: Option<String>,
    pub resource: Option<String>,
    pub context: Option<String>,
    pub original_prompt: Option<String>,
    pub created_at: Option<String>,
    pub workbench_id: Option<i64>,
    /// JOINed from workbenches.worktree
    pub worktree: Option<String>,
    pub pr_number: Option<i64>,
    pub worker_started_at: Option<String>,
    pub intervention_count: i64,
    pub captain_review_trigger: Option<String>,
    pub session_ids: String,
    pub last_activity_at: Option<String>,
    pub plan: Option<String>,
    pub no_pr: i64,
    pub no_auto_merge: i64,
    pub planning: i64,
    pub worker_seq: i64,
    pub reopen_seq: i64,
    pub reopened_at: Option<String>,
    pub reopen_source: Option<String>,
    pub images: Option<String>,
    pub review_fail_count: i64,
    pub clarifier_fail_count: i64,
    pub spawn_fail_count: i64,
    pub merge_fail_count: i64,
    pub escalation_report: Option<String>,
    pub source: Option<String>,
    pub rev: i64,
    pub paused_until: Option<i64>,
    /// JOINed from projects.github_repo
    pub github_repo: Option<String>,
}

impl TaskRow {
    pub fn into_task(self) -> Result<Task> {
        let status: ItemStatus = self.status.parse().map_err(|e| {
            anyhow::anyhow!("task {} has unknown status {:?}: {e}", self.id, self.status)
        })?;
        let captain_review_trigger = match self.captain_review_trigger {
            Some(s) => Some(s.parse().map_err(|e| {
                anyhow::anyhow!(
                    "task {} has unknown captain_review_trigger {s:?}: {e}",
                    self.id,
                )
            })?),
            None => None,
        };
        let session_ids = SessionIds::from_json(&self.session_ids)
            .with_context(|| format!("task {} has invalid session_ids JSON", self.id))?;
        Ok(Task {
            id: self.id,
            title: self.title,
            status,
            project_id: self.project_id,
            project: self.project,
            worker: self.worker,
            resource: self.resource,
            context: self.context,
            original_prompt: self.original_prompt,
            created_at: self.created_at,
            workbench_id: self.workbench_id.unwrap_or(0),
            worktree: self.worktree,
            branch: None,
            pr_number: self.pr_number,
            worker_started_at: self.worker_started_at,
            intervention_count: self.intervention_count,
            captain_review_trigger,
            session_ids,
            last_activity_at: self.last_activity_at,
            plan: self.plan,
            no_pr: self.no_pr != 0,
            no_auto_merge: self.no_auto_merge != 0,
            planning: self.planning != 0,
            worker_seq: self.worker_seq,
            reopen_seq: self.reopen_seq,
            reopened_at: self.reopened_at,
            reopen_source: self.reopen_source,
            images: self.images,
            review_fail_count: self.review_fail_count,
            clarifier_fail_count: self.clarifier_fail_count,
            spawn_fail_count: self.spawn_fail_count,
            merge_fail_count: self.merge_fail_count,
            escalation_report: self.escalation_report,
            source: self.source,
            rev: self.rev,
            paused_until: self.paused_until,
            github_repo: self.github_repo,
            rebase_worker: None,
            rebase_retries: 0,
            rebase_head_sha: None,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct RoutingRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub project_id: i64,
    pub project: String,
    pub worker: Option<String>,
    pub resource: Option<String>,
}

impl RoutingRow {
    pub fn into_routing(self) -> Result<TaskRouting> {
        let status: ItemStatus = self.status.parse().map_err(|e| {
            anyhow::anyhow!(
                "routing row {} has unknown status {:?}: {e}",
                self.id,
                self.status,
            )
        })?;
        Ok(TaskRouting {
            id: self.id,
            title: self.title,
            status,
            project_id: self.project_id,
            project: self.project,
            worker: self.worker,
            resource: self.resource,
        })
    }
}
