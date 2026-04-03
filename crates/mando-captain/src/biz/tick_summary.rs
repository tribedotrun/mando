//! TickResult formatting — pure helpers.

use mando_types::captain::TickResult;

/// Format a TickResult for CLI display with per-action breakdown.
#[cfg(test)]
pub(crate) fn format_tick_result(result: &TickResult) -> String {
    use mando_types::captain::{ActionKind, TickMode};
    let mut lines = Vec::new();

    lines.push(format!(
        "Captain tick ({}) — {}/{} workers",
        result.mode, result.active_workers, result.max_workers
    ));

    // Per-action breakdown (show each action, not summary stats)
    if !result.dry_actions.is_empty() {
        let label = if result.mode == TickMode::DryRun {
            "Dry-run actions:"
        } else {
            "Actions:"
        };
        lines.push(label.into());
        for da in &result.dry_actions {
            let verb = match da.action {
                ActionKind::Skip => "Skipped",
                ActionKind::Nudge => "Dispatched",
                ActionKind::Ship => "Done",
                ActionKind::CaptainReview => "Reviewing",
            };
            let detail = da.reason.as_deref().or(da.message.as_deref()).unwrap_or("");
            if detail.is_empty() {
                lines.push(format!("  {verb}: {}", da.worker));
            } else {
                lines.push(format!("  {verb}: {} — {detail}", da.worker));
            }
        }
    }

    if !result.tasks.is_empty() {
        let mut pairs: Vec<_> = result.tasks.iter().collect();
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let summary = pairs
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Tasks: [{}]", summary));
    }

    if !result.alerts.is_empty() {
        lines.push(format!("Alerts ({}):", result.alerts.len()));
        for alert in &result.alerts {
            lines.push(format!("  - {}", alert));
        }
    }

    if let Some(ref err) = result.error {
        lines.push(format!("Error: {}", err));
    }

    lines.join("\n")
}

/// Format TickResult as JSON value for API responses.
pub(crate) fn tick_result_to_json(result: &TickResult) -> serde_json::Value {
    serde_json::to_value(result).unwrap_or_else(|e| {
        tracing::warn!(module = "tick_summary", error = %e, "failed to serialize tick result");
        serde_json::json!({"error": format!("serialize failed: {e}")})
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mando_types::captain::TickMode;
    use std::collections::HashMap;

    #[test]
    fn format_basic() {
        let result = TickResult {
            mode: TickMode::Live,
            tick_id: None,
            max_workers: 10,
            active_workers: 3,
            tasks: {
                let mut m = HashMap::new();
                m.insert("new".into(), 2);
                m.insert("in-progress".into(), 3);
                m
            },
            alerts: vec!["Worker crashed".into()],
            dry_actions: vec![],
            error: None,
            rate_limited: false,
        };
        let formatted = format_tick_result(&result);
        assert!(formatted.contains("Captain tick (live)"));
        assert!(formatted.contains("3/10 workers"));
        assert!(formatted.contains("new=2"));
        assert!(formatted.contains("Worker crashed"));
    }

    #[test]
    fn format_dry_run() {
        use mando_types::captain::{Action, ActionKind};
        let result = TickResult {
            mode: TickMode::DryRun,
            tick_id: None,
            max_workers: 10,
            active_workers: 0,
            tasks: HashMap::new(),
            alerts: vec![],
            dry_actions: vec![Action {
                worker: "w1".into(),
                action: ActionKind::Nudge,
                message: None,
                reason: Some("would spawn worker".into()),
            }],
            error: None,
            rate_limited: false,
        };
        let formatted = format_tick_result(&result);
        assert!(formatted.contains("dry-run"));
        assert!(formatted.contains("would spawn worker"));
    }

    #[test]
    fn format_error() {
        let result = TickResult {
            mode: TickMode::Live,
            tick_id: None,
            max_workers: 0,
            active_workers: 0,
            tasks: HashMap::new(),
            alerts: vec![],
            dry_actions: vec![],
            error: Some("lock held".into()),
            rate_limited: false,
        };
        let formatted = format_tick_result(&result);
        assert!(formatted.contains("Error: lock held"));
    }

    #[test]
    fn format_per_action_breakdown() {
        use mando_types::captain::{Action, ActionKind};
        let result = TickResult {
            mode: TickMode::Live,
            tick_id: None,
            max_workers: 5,
            active_workers: 3,
            tasks: HashMap::new(),
            alerts: vec![],
            dry_actions: vec![
                Action {
                    worker: "item-1".into(),
                    action: ActionKind::Nudge,
                    message: None,
                    reason: Some("spawned worker".into()),
                },
                Action {
                    worker: "item-2".into(),
                    action: ActionKind::Ship,
                    message: None,
                    reason: Some("PR #456 ready".into()),
                },
                Action {
                    worker: "item-3".into(),
                    action: ActionKind::CaptainReview,
                    message: None,
                    reason: Some("needs review".into()),
                },
            ],
            error: None,
            rate_limited: false,
        };
        let formatted = format_tick_result(&result);
        assert!(
            formatted.contains("Dispatched: item-1"),
            "should show Dispatched"
        );
        assert!(formatted.contains("Done: item-2"), "should show Done");
        assert!(
            formatted.contains("Reviewing: item-3"),
            "should show Reviewing"
        );
        // Per-action breakdown, not summary stats
        assert!(
            !formatted.contains("Dry-run actions:"),
            "live mode should say Actions:"
        );
        assert!(formatted.contains("Actions:"));
    }

    #[test]
    fn to_json_round_trip() {
        let result = TickResult {
            mode: TickMode::Live,
            tick_id: Some("abc12345".into()),
            max_workers: 10,
            active_workers: 3,
            tasks: HashMap::new(),
            alerts: vec![],
            dry_actions: vec![],
            error: None,
            rate_limited: false,
        };
        let val = tick_result_to_json(&result);
        assert_eq!(val["mode"], "live");
        assert_eq!(val["max_workers"], 10);
    }
}
