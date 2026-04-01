//! SQLite-backed decision journal for captain self-improvement.
//!
//! Thin async wrapper around mando_db::queries::journal.

use anyhow::Result;
use sqlx::SqlitePool;

use super::journal_types::{DecisionEntry, DecisionInput, Pattern, StateSnapshot};
use mando_db::queries::journal as jq;

/// Handle to the captain decision journal database.
pub struct JournalDb {
    pool: SqlitePool,
}

impl JournalDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(crate) async fn log_decision(&self, input: &DecisionInput<'_>) -> Result<()> {
        let state_json = serde_json::to_string(input.state)?;
        let source_str = input.source.to_string();
        jq::log_decision(
            &self.pool,
            &jq::DecisionInput {
                tick_id: input.tick_id,
                worker: input.worker,
                item_id: input.item_id,
                action: input.action,
                source: &source_str,
                rule: input.rule,
                state_json: &state_json,
            },
        )
        .await
    }

    pub(crate) async fn resolve_outcomes(
        &self,
        worker: &str,
        current_is_skip: bool,
    ) -> Result<usize> {
        jq::resolve_outcomes(&self.pool, worker, current_is_skip).await
    }

    pub(crate) async fn resolve_terminal(&self, worker: &str) -> Result<usize> {
        jq::resolve_terminal(&self.pool, worker).await
    }

    pub(crate) async fn unresolved_workers(&self) -> Result<Vec<String>> {
        jq::unresolved_workers(&self.pool).await
    }

    pub async fn recent_decisions(
        &self,
        worker: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DecisionEntry>> {
        let rows = jq::recent_decisions(&self.pool, worker, limit).await?;
        Ok(rows.into_iter().map(decision_row_to_entry).collect())
    }

    pub(crate) async fn stats_by_action_rule(
        &self,
        days: i64,
    ) -> Result<Vec<super::journal_types::ActionRuleStats>> {
        let db_stats = jq::stats_by_action_rule(&self.pool, days).await?;
        Ok(db_stats.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn escalation_stats(&self, days: i64) -> Result<Vec<(String, String, i64)>> {
        jq::escalation_stats(&self.pool, days).await
    }

    pub(crate) async fn repeat_failures(
        &self,
        days: i64,
        min_repeats: i64,
    ) -> Result<Vec<(String, String, i64)>> {
        jq::repeat_failures(&self.pool, days, min_repeats).await
    }

    pub async fn total_counts(&self) -> Result<(i64, i64, i64, i64)> {
        jq::total_counts(&self.pool).await
    }

    pub(crate) async fn insert_pattern(
        &self,
        pattern: &str,
        signal: &str,
        recommendation: &str,
        confidence: f64,
        sample_size: i64,
    ) -> Result<i64> {
        jq::insert_pattern(
            &self.pool,
            pattern,
            signal,
            recommendation,
            confidence,
            sample_size,
        )
        .await
    }

    pub async fn list_patterns(&self, status: Option<&str>) -> Result<Vec<Pattern>> {
        let rows = jq::list_patterns(&self.pool, status).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update_pattern_status(&self, id: i64, status: &str) -> Result<()> {
        jq::update_pattern_status(&self.pool, id, status).await
    }

    pub(crate) async fn prune(&self, retain_days: i64) -> Result<usize> {
        jq::prune(&self.pool, retain_days).await
    }
}

fn decision_row_to_entry(row: jq::DecisionRow) -> DecisionEntry {
    let state: StateSnapshot = serde_json::from_str(&row.state).unwrap_or_else(|e| {
        tracing::warn!(
            module = "journal",
            row_id = row.id,
            error = %e,
            "failed to deserialize decision state, using default"
        );
        StateSnapshot::default()
    });
    DecisionEntry {
        id: row.id,
        tick_id: row.tick_id,
        worker: row.worker,
        item_id: row.item_id,
        action: row.action,
        source: row.source,
        rule: row.rule,
        state,
        outcome: row.outcome,
        resolved_at: row.resolved_at,
        created_at: row.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::journal_types::DecisionSource;

    async fn test_db() -> JournalDb {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        JournalDb::new(db.pool().clone())
    }

    fn test_snapshot() -> StateSnapshot {
        StateSnapshot {
            process_alive: true,
            stream_stale_s: Some(500.0),
            seconds_active: 7200.0,
            intervention_count: 0,
            nudge_count: 1,
            no_pr: false,
            reopen_seq: 0,
            has_reopen_ack: true,
            branch_ahead: true,
            unresolved_threads: 0,
            unreplied_threads: 0,
            unaddressed_issue_comments: 0,
            pr_ci_status: Some("success".into()),
        }
    }

    #[tokio::test]
    async fn log_and_query() {
        let db = test_db().await;
        let snap = test_snapshot();
        db.log_decision(&DecisionInput {
            tick_id: "tick-1",
            worker: "worker-a",
            item_id: Some("item-1"),
            action: "nudge",
            source: DecisionSource::Deterministic,
            rule: "stream_stale",
            state: &snap,
        })
        .await
        .unwrap();

        let decisions = db.recent_decisions(None, 10).await.unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].worker, "worker-a");
        assert_eq!(decisions[0].action, "nudge");
        assert!(decisions[0].outcome.is_none());
    }

    #[tokio::test]
    async fn resolve_success() {
        let db = test_db().await;
        let snap = test_snapshot();
        db.log_decision(&DecisionInput {
            tick_id: "tick-1",
            worker: "worker-a",
            item_id: None,
            action: "nudge",
            source: DecisionSource::Deterministic,
            rule: "stream_stale",
            state: &snap,
        })
        .await
        .unwrap();

        let resolved = db.resolve_outcomes("worker-a", true).await.unwrap();
        assert_eq!(resolved, 1);

        let decisions = db.recent_decisions(None, 10).await.unwrap();
        assert_eq!(decisions[0].outcome.as_deref(), Some("success"));
    }

    #[tokio::test]
    async fn resolve_failure() {
        let db = test_db().await;
        let snap = test_snapshot();
        db.log_decision(&DecisionInput {
            tick_id: "tick-1",
            worker: "worker-a",
            item_id: None,
            action: "nudge",
            source: DecisionSource::Deterministic,
            rule: "stream_stale",
            state: &snap,
        })
        .await
        .unwrap();

        let resolved = db.resolve_outcomes("worker-a", false).await.unwrap();
        assert_eq!(resolved, 1);

        let decisions = db.recent_decisions(None, 10).await.unwrap();
        assert_eq!(decisions[0].outcome.as_deref(), Some("failure"));
    }

    #[tokio::test]
    async fn total_counts_empty_table() {
        let db = test_db().await;
        let (total, successes, failures, unresolved) = db.total_counts().await.unwrap();
        assert_eq!(total, 0);
        assert_eq!(successes, 0);
        assert_eq!(failures, 0);
        assert_eq!(unresolved, 0);
    }
}
