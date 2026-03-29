//! Captain decision journal queries.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

// Re-use existing types from mando-captain (they'll need to be moved or shared).
// For now we define lightweight query result types here.

/// A decision entry from the journal.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DecisionRow {
    pub id: i64,
    pub tick_id: String,
    pub worker: String,
    pub item_id: Option<String>,
    pub action: String,
    pub source: String,
    pub rule: String,
    pub state: String,
    pub outcome: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

/// Action/rule aggregation stats.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActionRuleStats {
    pub action: String,
    pub rule: String,
    pub total: i64,
    pub successes: i64,
    pub failures: i64,
    pub success_rate: f64,
}

/// A learned pattern.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PatternRow {
    pub id: i64,
    pub pattern: String,
    pub signal: String,
    pub recommendation: String,
    pub confidence: f64,
    pub sample_size: i64,
    pub status: String,
    pub created_at: String,
}

/// Input for logging a captain decision.
pub struct DecisionInput<'a> {
    pub tick_id: &'a str,
    pub worker: &'a str,
    pub item_id: Option<&'a str>,
    pub action: &'a str,
    pub source: &'a str,
    pub rule: &'a str,
    pub state_json: &'a str,
}

/// Log a decision.
pub async fn log_decision(pool: &SqlitePool, input: &DecisionInput<'_>) -> Result<()> {
    let now = mando_types::now_rfc3339();
    sqlx::query(
        "INSERT INTO task_decisions (tick_id, worker, item_id, action, source, rule, state, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.tick_id)
    .bind(input.worker)
    .bind(input.item_id)
    .bind(input.action)
    .bind(input.source)
    .bind(input.rule)
    .bind(input.state_json)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Resolve outcomes for a worker.
pub async fn resolve_outcomes(
    pool: &SqlitePool,
    worker: &str,
    current_is_skip: bool,
) -> Result<usize> {
    let outcome = if current_is_skip {
        "success"
    } else {
        "failure"
    };
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE task_decisions SET outcome = ?, resolved_at = ?
         WHERE worker = ? AND outcome IS NULL",
    )
    .bind(outcome)
    .bind(&now)
    .bind(worker)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() as usize)
}

/// Mark unresolved decisions as terminal.
pub async fn resolve_terminal(pool: &SqlitePool, worker: &str) -> Result<usize> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE task_decisions SET outcome = 'terminal', resolved_at = ?
         WHERE worker = ? AND outcome IS NULL",
    )
    .bind(&now)
    .bind(worker)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() as usize)
}

/// List workers with unresolved decisions.
pub async fn unresolved_workers(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT worker FROM task_decisions WHERE outcome IS NULL")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(w,)| w).collect())
}

/// Recent decisions, optionally by worker.
pub async fn recent_decisions(
    pool: &SqlitePool,
    worker: Option<&str>,
    limit: usize,
) -> Result<Vec<DecisionRow>> {
    let rows = if let Some(w) = worker {
        sqlx::query_as::<_, DecisionRow>(
            "SELECT id, tick_id, worker, item_id, action, source, rule, state,
                    outcome, resolved_at, created_at
             FROM task_decisions WHERE worker = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(w)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, DecisionRow>(
            "SELECT id, tick_id, worker, item_id, action, source, rule, state,
                    outcome, resolved_at, created_at
             FROM task_decisions ORDER BY id DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Stats by action/rule pair.
pub async fn stats_by_action_rule(pool: &SqlitePool, days: i64) -> Result<Vec<ActionRuleStats>> {
    let cutoff = super::cutoff_rfc3339(days);
    let rows: Vec<(String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT action, rule,
                COUNT(*) as total,
                SUM(CASE WHEN outcome = 'success' THEN 1 ELSE 0 END) as successes,
                SUM(CASE WHEN outcome = 'failure' THEN 1 ELSE 0 END) as failures
         FROM task_decisions
         WHERE created_at > ? AND outcome IS NOT NULL
         GROUP BY action, rule
         HAVING total >= 5
         ORDER BY total DESC",
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await
    .context("failed to aggregate action/rule stats")?;

    Ok(rows
        .into_iter()
        .map(|(action, rule, total, successes, failures)| {
            let rate = if total > 0 {
                successes as f64 / total as f64
            } else {
                0.0
            };
            ActionRuleStats {
                action,
                rule,
                total,
                successes,
                failures,
                success_rate: rate,
            }
        })
        .collect())
}

/// Escalation chain stats.
pub async fn escalation_stats(pool: &SqlitePool, days: i64) -> Result<Vec<(String, String, i64)>> {
    let cutoff = super::cutoff_rfc3339(days);
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT d1.action AS first_action, d2.action AS next_action, COUNT(*) as cnt
         FROM task_decisions d1
         JOIN task_decisions d2 ON d1.worker = d2.worker
            AND d2.id = (SELECT MIN(id) FROM task_decisions d3
                         WHERE d3.worker = d1.worker AND d3.id > d1.id)
         WHERE d1.outcome = 'failure' AND d1.created_at > ?
         GROUP BY d1.action, d2.action
         ORDER BY cnt DESC",
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await
    .context("failed to query escalation stats")?;
    Ok(rows)
}

/// Repeat failures.
pub async fn repeat_failures(
    pool: &SqlitePool,
    days: i64,
    min_repeats: i64,
) -> Result<Vec<(String, String, i64)>> {
    let cutoff = super::cutoff_rfc3339(days);
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT worker, action, COUNT(*) as cnt
         FROM task_decisions
         WHERE outcome = 'failure' AND created_at > ?
         GROUP BY worker, action
         HAVING cnt >= ?
         ORDER BY cnt DESC",
    )
    .bind(&cutoff)
    .bind(min_repeats)
    .fetch_all(pool)
    .await
    .context("failed to query repeat failures")?;
    Ok(rows)
}

/// Total decision counts.
pub async fn total_counts(pool: &SqlitePool) -> Result<(i64, i64, i64, i64)> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            COUNT(*) as total,
            COALESCE(SUM(CASE WHEN outcome = 'success' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN outcome = 'failure' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN outcome IS NULL THEN 1 ELSE 0 END), 0)
         FROM task_decisions",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Insert a pattern.
pub async fn insert_pattern(
    pool: &SqlitePool,
    pattern: &str,
    signal: &str,
    recommendation: &str,
    confidence: f64,
    sample_size: i64,
) -> Result<i64> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "INSERT INTO task_patterns (pattern, signal, recommendation, confidence, sample_size, status, created_at)
         VALUES (?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(pattern)
    .bind(signal)
    .bind(recommendation)
    .bind(confidence)
    .bind(sample_size)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// List patterns.
pub async fn list_patterns(pool: &SqlitePool, status: Option<&str>) -> Result<Vec<PatternRow>> {
    let rows = if let Some(s) = status {
        sqlx::query_as::<_, PatternRow>(
            "SELECT id, pattern, signal, recommendation, confidence, sample_size, status, created_at
             FROM task_patterns WHERE status = ? ORDER BY id DESC",
        )
        .bind(s)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, PatternRow>(
            "SELECT id, pattern, signal, recommendation, confidence, sample_size, status, created_at
             FROM task_patterns ORDER BY id DESC",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Update pattern status.
pub async fn update_pattern_status(pool: &SqlitePool, id: i64, status: &str) -> Result<()> {
    let result = sqlx::query("UPDATE task_patterns SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    anyhow::ensure!(result.rows_affected() > 0, "pattern {id} not found");
    Ok(())
}

/// Prune old decisions.
pub async fn prune(pool: &SqlitePool, retain_days: i64) -> Result<usize> {
    let cutoff = super::cutoff_rfc3339(retain_days);
    let result = sqlx::query("DELETE FROM task_decisions WHERE created_at < ?")
        .bind(&cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() as usize)
}
