//! Analytics aggregate queries — task throughput and success metrics.

use anyhow::Result;
use sqlx::SqlitePool;

/// Task counts grouped by status bucket.
#[derive(Debug, Default, serde::Serialize)]
pub struct TaskCounts {
    pub total: i64,
    pub merged: i64,
    pub completed_no_pr: i64,
    pub errored: i64,
    pub escalated: i64,
    pub canceled: i64,
    pub in_progress: i64,
    pub queued: i64,
    pub action_needed: i64,
}

/// Per-project task breakdown.
#[derive(Debug, serde::Serialize)]
pub struct ProjectTasks {
    pub project: String,
    pub total: i64,
    pub merged: i64,
    pub errored: i64,
}

/// Full analytics response.
#[derive(Debug, serde::Serialize)]
pub struct AnalyticsData {
    pub task_counts: TaskCounts,
    pub project_tasks: Vec<ProjectTasks>,
}

pub async fn fetch_analytics(pool: &SqlitePool) -> Result<AnalyticsData> {
    let task_counts = fetch_task_counts(pool).await?;
    let project_tasks = fetch_project_tasks(pool).await?;

    Ok(AnalyticsData {
        task_counts,
        project_tasks,
    })
}

async fn fetch_task_counts(pool: &SqlitePool) -> Result<TaskCounts> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
        .fetch_one(pool)
        .await?;

    let counts: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, COUNT(*) FROM tasks GROUP BY status")
            .fetch_all(pool)
            .await?;

    let mut tc = TaskCounts {
        total,
        ..Default::default()
    };
    for (status, n) in counts {
        match status.as_str() {
            "merged" => tc.merged = n,
            "completed-no-pr" => tc.completed_no_pr = n,
            "errored" => tc.errored = n,
            "escalated" => tc.escalated = n,
            "canceled" => tc.canceled = n,
            "in-progress" | "captain-reviewing" | "clarifying" => tc.in_progress += n,
            "queued" | "new" | "rework" => tc.queued += n,
            "awaiting-review" | "needs-clarification" | "handed-off" => tc.action_needed += n,
            _ => {}
        }
    }
    Ok(tc)
}

async fn fetch_project_tasks(pool: &SqlitePool) -> Result<Vec<ProjectTasks>> {
    let rows: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT COALESCE(project, '(no project)') as proj,
                COUNT(*),
                SUM(CASE WHEN status IN ('merged', 'completed-no-pr') THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('errored', 'escalated') THEN 1 ELSE 0 END)
         FROM tasks
         WHERE project IS NOT NULL
         GROUP BY proj
         ORDER BY 2 DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(project, total, merged, errored)| ProjectTasks {
            project,
            total,
            merged,
            errored,
        })
        .collect())
}
