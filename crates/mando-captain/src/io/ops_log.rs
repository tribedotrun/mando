//! Write-ahead intent log (WAL) for crash recovery.
//!
//! Single-writer JSON-backed log. Each multi-step operation is tracked
//! from `begin()` to `complete()`. On crash, `incomplete()` returns
//! operations that need to be resumed.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Maximum age for WAL entries before pruning (72 hours).
pub const STALE_AGE_SECS: u64 = 72 * 3600;

/// A single WAL entry representing an in-flight operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpEntry {
    pub op_id: String,
    pub op_type: String,
    pub params: serde_json::Value,
    pub steps_completed: Vec<String>,
    pub started_at: String,
}

/// The WAL store — a list of in-flight operations.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OpsLog {
    pub entries: Vec<OpEntry>,
}

/// WAL file path: `~/.mando/state/ops-log.json`.
pub(crate) fn ops_log_path() -> PathBuf {
    mando_config::state_dir().join("ops-log.json")
}

/// Load the WAL from disk.
///
/// Uses error-level logging (not warn) because WAL corruption means
/// in-flight operations are lost and cannot be recovered.
pub(crate) fn load_ops_log(path: &Path) -> OpsLog {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::error!(
                module = "wal",
                path = %path.display(),
                error = %e,
                "WAL file corrupt — in-flight operations lost, starting empty"
            );
            OpsLog::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => OpsLog::default(),
        Err(e) => {
            tracing::error!(
                module = "wal",
                path = %path.display(),
                error = %e,
                "WAL file unreadable — in-flight operations may be lost"
            );
            OpsLog::default()
        }
    }
}

/// Save the WAL to disk.
pub(crate) fn save_ops_log(log: &OpsLog, path: &Path) -> Result<()> {
    mando_shared::save_json_file(path, log)
}

/// Begin a new operation. Returns the op_id.
#[cfg(test)]
pub(crate) fn begin_op(log: &mut OpsLog, op_type: &str, params: serde_json::Value) -> String {
    let op_id = mando_uuid::Uuid::v4().to_string();
    log.entries.push(OpEntry {
        op_id: op_id.clone(),
        op_type: op_type.to_string(),
        params,
        steps_completed: Vec::new(),
        started_at: mando_types::now_rfc3339(),
    });
    op_id
}

/// Mark a step as completed (idempotent).
pub(crate) fn mark_step(log: &mut OpsLog, op_id: &str, step: &str) {
    if let Some(entry) = log.entries.iter_mut().find(|e| e.op_id == op_id) {
        if !entry.steps_completed.contains(&step.to_string()) {
            entry.steps_completed.push(step.to_string());
        }
    }
}

/// Complete an operation — remove it from the WAL.
pub(crate) fn complete_op(log: &mut OpsLog, op_id: &str) {
    log.entries.retain(|e| e.op_id != op_id);
}

/// Abandon an operation — remove it from the WAL with a logged reason.
///
/// Use this for permanent failures that should never be retried (e.g.
/// "auto merge not allowed", missing permissions). Without this, failed
/// operations stay in the WAL and get blindly retried on every restart
/// until the 72-hour prune kicks in.
pub(crate) fn abandon_op(log: &mut OpsLog, op_id: &str, reason: &str) {
    let before = log.entries.len();
    log.entries.retain(|e| e.op_id != op_id);
    if log.entries.len() < before {
        tracing::warn!(
            module = "wal",
            op_id = %op_id,
            reason = %reason,
            "abandoning WAL operation — permanent failure, will not retry"
        );
    } else {
        tracing::debug!(
            module = "wal",
            op_id = %op_id,
            "abandon_op: no entry found (already removed?)"
        );
    }
}

/// Get all incomplete operations.
pub(crate) fn incomplete_ops(log: &OpsLog) -> &[OpEntry] {
    &log.entries
}

/// Prune entries older than `max_age_secs`.
pub(crate) fn prune_stale(log: &mut OpsLog, max_age_secs: u64) {
    use time::format_description::well_known::Rfc3339;
    let now = time::OffsetDateTime::now_utc();
    let cutoff = now.unix_timestamp() - max_age_secs as i64;
    log.entries.retain(|e| {
        time::OffsetDateTime::parse(&e.started_at, &Rfc3339)
            .map(|dt| dt.unix_timestamp() > cutoff)
            .unwrap_or(true) // Keep entries we can't parse.
    });
}

/// Check if a step has been completed for an operation.
pub(crate) fn is_step_done(log: &OpsLog, op_id: &str, step: &str) -> bool {
    log.entries
        .iter()
        .find(|e| e.op_id == op_id)
        .is_some_and(|e| e.steps_completed.iter().any(|s| s == step))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_mark_complete() {
        let mut log = OpsLog::default();
        let id = begin_op(&mut log, "merge", serde_json::json!({"pr": "123"}));
        assert_eq!(log.entries.len(), 1);

        mark_step(&mut log, &id, "check_ci");
        assert!(is_step_done(&log, &id, "check_ci"));
        assert!(!is_step_done(&log, &id, "squash_merge"));

        // Idempotent mark.
        mark_step(&mut log, &id, "check_ci");
        assert_eq!(log.entries[0].steps_completed.len(), 1);

        complete_op(&mut log, &id);
        assert!(log.entries.is_empty());
    }

    #[test]
    fn abandon_removes_entry() {
        let mut log = OpsLog::default();
        let id = begin_op(&mut log, "merge", serde_json::json!({"pr": "42"}));
        mark_step(&mut log, &id, "check_merged");
        assert_eq!(log.entries.len(), 1);

        abandon_op(&mut log, &id, "auto merge not allowed");
        assert!(log.entries.is_empty());
    }

    #[test]
    fn reconcile_merge_gate_blocks_when_pr_not_merged() {
        // Simulates: check_merged done, but squash_merge NOT done (PR is still open).
        // The reconciler should abandon, not proceed to update_task.
        let mut log = OpsLog::default();
        let id = begin_op(
            &mut log,
            "merge",
            serde_json::json!({"pr": "334", "repo": "owner/repo", "item_id": "2"}),
        );

        // Step 1 done: we checked GitHub and PR was NOT merged.
        mark_step(&mut log, &id, "check_merged");
        assert!(is_step_done(&log, &id, "check_merged"));
        assert!(!is_step_done(&log, &id, "squash_merge"));

        // Gate: squash_merge not done → should abandon.
        if !is_step_done(&log, &id, "squash_merge") {
            abandon_op(&mut log, &id, "PR #334 not merged on GitHub");
        }
        assert!(log.entries.is_empty(), "WAL entry should be abandoned");
    }

    #[test]
    fn reconcile_merge_gate_proceeds_when_pr_merged() {
        // Simulates: check_merged done AND squash_merge done (PR is merged on GitHub).
        // The reconciler should proceed to update_task.
        let mut log = OpsLog::default();
        let id = begin_op(
            &mut log,
            "merge",
            serde_json::json!({"pr": "334", "repo": "owner/repo", "item_id": "2"}),
        );

        // Step 1 done: we checked GitHub and PR WAS merged.
        mark_step(&mut log, &id, "squash_merge");
        mark_step(&mut log, &id, "check_merged");
        assert!(is_step_done(&log, &id, "squash_merge"));

        // Gate passes → proceed (entry still in WAL until update_task completes).
        assert_eq!(log.entries.len(), 1);
        assert!(!is_step_done(&log, &id, "update_task"));
    }

    #[test]
    fn incomplete_returns_unfinished() {
        let mut log = OpsLog::default();
        begin_op(&mut log, "merge", serde_json::json!({}));
        begin_op(&mut log, "learn", serde_json::json!({}));

        assert_eq!(incomplete_ops(&log).len(), 2);
    }

    #[test]
    fn save_load_round_trip() {
        let tmp = std::env::temp_dir().join("mando-test-opslog.json");
        let mut log = OpsLog::default();
        begin_op(&mut log, "test", serde_json::json!({"foo": "bar"}));
        save_ops_log(&log, &tmp).unwrap();

        let loaded = load_ops_log(&tmp);
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].op_type, "test");

        std::fs::remove_file(&tmp).ok();
    }
}
