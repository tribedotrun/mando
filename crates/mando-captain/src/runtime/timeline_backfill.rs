//! Timeline backfill — reconstruct timeline from task fields + session log.

use mando_types::session::SessionEntry;
use mando_types::timeline::{TimelineEvent, TimelineEventType};
use mando_types::Task;

/// Backfill timeline for an item if not already done.
///
/// Checks for backfill marker in DB. If absent, reconstructs events from
/// task fields and the CC session log.
pub(crate) async fn backfill_if_needed(item: &Task, pool: &sqlx::SqlitePool) {
    let task_id = item.id;

    match mando_db::queries::timeline::has_backfill_marker(pool, task_id).await {
        Ok(true) => return, // Already backfilled.
        Ok(false) => {}     // Need to backfill.
        Err(e) => {
            tracing::warn!(
                module = "timeline-backfill",
                task_id = task_id,
                error = %e,
                "failed to check backfill marker, skipping backfill"
            );
            return;
        }
    }

    let existing = mando_db::queries::timeline::load(pool, task_id)
        .await
        .unwrap_or_default();

    let item_id_str = item.id.to_string();
    let sessions = load_item_sessions(pool, &item_id_str).await;
    let events = build_events_for_item(item, &sessions);

    // Deduplicate by (event_type, session_id).
    // SessionResumed and WorkerNudged are siblings — if either exists for a
    // session, the backfill counterpart should be suppressed.
    let mut existing_keys: std::collections::HashSet<(String, String)> = existing
        .iter()
        .filter_map(|e| {
            e.data
                .get("session_id")
                .and_then(|s| s.as_str())
                .map(|sid| (format!("{:?}", e.event_type), sid.to_string()))
        })
        .collect();
    // Expand siblings: SessionResumed ↔ WorkerNudged.
    let siblings: Vec<(String, String)> = existing_keys
        .iter()
        .filter_map(|(et, sid)| {
            let sibling = if et == &format!("{:?}", TimelineEventType::SessionResumed) {
                format!("{:?}", TimelineEventType::WorkerNudged)
            } else if et == &format!("{:?}", TimelineEventType::WorkerNudged) {
                format!("{:?}", TimelineEventType::SessionResumed)
            } else {
                return None;
            };
            Some((sibling, sid.clone()))
        })
        .collect();
    existing_keys.extend(siblings);
    let has_created = existing
        .iter()
        .any(|e| e.event_type == TimelineEventType::Created);

    let mut new_events = Vec::new();
    for event in events {
        if event.event_type == TimelineEventType::Created && has_created {
            continue;
        }
        if let Some(sid) = event.data.get("session_id").and_then(|s| s.as_str()) {
            let key = (format!("{:?}", event.event_type), sid.to_string());
            if existing_keys.contains(&key) {
                continue;
            }
        }
        new_events.push(event);
    }

    // Always add a backfill marker.
    new_events.push(TimelineEvent {
        event_type: TimelineEventType::StatusChanged,
        timestamp: String::new(),
        actor: "system".into(),
        summary: String::new(),
        data: serde_json::json!({"source": "backfill"}),
    });

    if let Err(e) = mando_db::queries::timeline::bulk_insert(pool, task_id, &new_events).await {
        tracing::warn!(
            module = "timeline-backfill",
            task_id = task_id,
            error = %e,
            "failed to bulk insert backfill events"
        );
    }
}

/// Build timeline events for an item from its fields + session data.
fn build_events_for_item(item: &Task, sessions: &[SessionEntry]) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    // Created event.
    if let Some(ref ts) = item.created_at {
        events.push(TimelineEvent {
            event_type: TimelineEventType::Created,
            timestamp: ts.clone(),
            actor: "captain".into(),
            summary: format!("Item created: {}", item.title),
            data: serde_json::json!({"source": "backfill"}),
        });
    }

    // Build events from session log.
    for session in sessions {
        if session.session_id.is_empty() {
            continue;
        }

        let (event_type, summary) = session_to_event_type(session);
        events.push(TimelineEvent {
            event_type,
            timestamp: session.ts.clone(),
            actor: "captain".into(),
            summary,
            data: serde_json::json!({
                "source": "backfill",
                "session_id": session.session_id,
                "worker": session.worker_name,
            }),
        });
    }

    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    events
}

/// Map a session entry to a timeline event type + summary.
fn session_to_event_type(session: &SessionEntry) -> (TimelineEventType, String) {
    match session.caller.as_str() {
        "clarifier" => (
            TimelineEventType::ClarifyResolved,
            "Clarification session".into(),
        ),
        "worker" => {
            if session.status == "failed" || session.status == "timeout" {
                (
                    TimelineEventType::Escalated,
                    format!("Worker {} failed", session.worker_name),
                )
            } else if session.resumed {
                (
                    TimelineEventType::WorkerNudged,
                    format!("Resumed {}", session.worker_name),
                )
            } else {
                (
                    TimelineEventType::WorkerSpawned,
                    format!("Spawned {}", session.worker_name),
                )
            }
        }
        _ => (
            TimelineEventType::StatusChanged,
            format!("{} session", session.caller),
        ),
    }
}

/// Load sessions for a single task from the unified DB.
async fn load_item_sessions(pool: &sqlx::SqlitePool, task_id: &str) -> Vec<SessionEntry> {
    match mando_db::queries::sessions::list_sessions_for_task(pool, task_id).await {
        Ok(rows) => rows.into_iter().map(session_row_to_entry).collect(),
        Err(e) => {
            tracing::warn!(module = "timeline-backfill", error = %e, "failed to load sessions for task");
            Vec::new()
        }
    }
}

fn session_row_to_entry(row: mando_db::queries::sessions::SessionRow) -> SessionEntry {
    SessionEntry {
        session_id: row.session_id,
        ts: row.created_at,
        cwd: row.cwd,
        model: row.model,
        caller: row.caller,
        resumed: row.resumed != 0,
        source: "live".to_string(),
        cost_usd: row.cost_usd,
        duration_ms: row.duration_ms,
        title: String::new(),
        project: String::new(),
        task_id: row.task_id.unwrap_or_default(),
        worker_name: row.worker_name.unwrap_or_default(),
        status: row.status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: TimelineEventType, session_id: Option<&str>) -> TimelineEvent {
        let data = match session_id {
            Some(sid) => serde_json::json!({"session_id": sid, "source": "backfill"}),
            None => serde_json::json!({"source": "backfill"}),
        };
        TimelineEvent {
            event_type,
            timestamp: "2026-01-01T00:00:00Z".into(),
            actor: "captain".into(),
            summary: "test".into(),
            data,
        }
    }

    fn make_real_event(event_type: TimelineEventType, session_id: Option<&str>) -> TimelineEvent {
        let data = match session_id {
            Some(sid) => serde_json::json!({"session_id": sid, "worker": "w1"}),
            None => serde_json::json!({}),
        };
        TimelineEvent {
            event_type,
            timestamp: "2026-01-01T00:00:00Z".into(),
            actor: "captain".into(),
            summary: "real".into(),
            data,
        }
    }

    #[test]
    fn backfill_deduplicates_by_event_type_and_session_id() {
        let existing = vec![
            make_real_event(TimelineEventType::Created, None),
            make_real_event(TimelineEventType::WorkerSpawned, Some("sess-a")),
            make_real_event(TimelineEventType::AwaitingReview, Some("sess-a")),
        ];

        let backfill_events = vec![
            make_event(TimelineEventType::Created, None),
            make_event(TimelineEventType::WorkerSpawned, Some("sess-a")),
            make_event(TimelineEventType::WorkerSpawned, Some("sess-b")),
        ];

        let existing_keys: std::collections::HashSet<(String, String)> = existing
            .iter()
            .filter_map(|e| {
                e.data
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .map(|sid| (format!("{:?}", e.event_type), sid.to_string()))
            })
            .collect();
        let has_created = existing
            .iter()
            .any(|e| e.event_type == TimelineEventType::Created);

        let mut result = existing.clone();
        for event in backfill_events {
            if event.event_type == TimelineEventType::Created && has_created {
                continue;
            }
            if let Some(sid) = event.data.get("session_id").and_then(|s| s.as_str()) {
                let key = (format!("{:?}", event.event_type), sid.to_string());
                if existing_keys.contains(&key) {
                    continue;
                }
            }
            result.push(event);
        }

        assert_eq!(
            result.len(),
            4,
            "should deduplicate Created and WorkerSpawned(sess-a)"
        );
        assert_eq!(
            result.iter().filter(|e| e.summary == "real").count(),
            3,
            "all 3 real events preserved"
        );
        assert_eq!(
            result.iter().filter(|e| e.summary == "test").count(),
            1,
            "only sess-b backfill added"
        );
    }

    #[test]
    fn backfill_dedup_sibling_session_resumed_vs_worker_nudged() {
        // Real-time emitted SessionResumed for sess-c (from reopen).
        // Backfill would generate WorkerNudged for the same session.
        // Sibling expansion should suppress the duplicate.
        let existing = vec![make_real_event(
            TimelineEventType::SessionResumed,
            Some("sess-c"),
        )];

        let backfill_events = vec![make_event(TimelineEventType::WorkerNudged, Some("sess-c"))];

        let mut existing_keys: std::collections::HashSet<(String, String)> = existing
            .iter()
            .filter_map(|e| {
                e.data
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .map(|sid| (format!("{:?}", e.event_type), sid.to_string()))
            })
            .collect();
        let siblings: Vec<(String, String)> = existing_keys
            .iter()
            .filter_map(|(et, sid)| {
                let sibling = if et == &format!("{:?}", TimelineEventType::SessionResumed) {
                    format!("{:?}", TimelineEventType::WorkerNudged)
                } else if et == &format!("{:?}", TimelineEventType::WorkerNudged) {
                    format!("{:?}", TimelineEventType::SessionResumed)
                } else {
                    return None;
                };
                Some((sibling, sid.clone()))
            })
            .collect();
        existing_keys.extend(siblings);

        let mut result = existing.clone();
        for event in backfill_events {
            if let Some(sid) = event.data.get("session_id").and_then(|s| s.as_str()) {
                let key = (format!("{:?}", event.event_type), sid.to_string());
                if existing_keys.contains(&key) {
                    continue;
                }
            }
            result.push(event);
        }

        assert_eq!(
            result.len(),
            1,
            "WorkerNudged backfill suppressed by sibling SessionResumed"
        );
    }
}
