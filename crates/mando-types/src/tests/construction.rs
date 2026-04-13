//! Construction tests -- every type can be instantiated.

use std::collections::HashMap;

use crate::task::{FINALIZED, REOPENABLE, REWORKABLE};
use crate::{
    Action, ActionKind, AskHistoryEntry, BusEvent, ItemStatus, NotifyLevel, ScoutItem, ScoutStatus,
    SessionEntry, Task, TickMode, TickResult, TimelineEvent, TimelineEventType, WorkerContext,
};

#[test]
fn construct_task() {
    let item = Task::new("Fix the flux capacitor");
    assert_eq!(item.title, "Fix the flux capacitor");
    assert_eq!(item.status, ItemStatus::New);
}

#[test]
fn construct_item_status_all_variants() {
    let statuses = [
        ItemStatus::New,
        ItemStatus::Clarifying,
        ItemStatus::NeedsClarification,
        ItemStatus::Queued,
        ItemStatus::InProgress,
        ItemStatus::CaptainReviewing,
        ItemStatus::CaptainMerging,
        ItemStatus::AwaitingReview,
        ItemStatus::Rework,
        ItemStatus::HandedOff,
        ItemStatus::Escalated,
        ItemStatus::Errored,
        ItemStatus::Merged,
        ItemStatus::CompletedNoPr,
        ItemStatus::Canceled,
    ];
    assert_eq!(statuses.len(), 15);
}

#[test]
fn construct_scout_item() {
    let item = ScoutItem {
        id: 42,
        url: "https://example.com".into(),
        item_type: "article".into(),
        title: Some("Great article".into()),
        status: ScoutStatus::Pending,
        relevance: Some(8),
        quality: Some(7),
        date_added: "2026-03-16".into(),
        date_processed: None,
        added_by: Some("bot".into()),
        error_count: 0,
        source_name: None,
        date_published: Some("2026-03-10".into()),
        rev: 1,
        research_run_id: None,
    };
    assert_eq!(item.id, 42);
    assert_eq!(item.status, ScoutStatus::Pending);
}

#[test]
fn construct_worker_context() {
    let ctx = WorkerContext {
        session_name: "worker-1-1".into(),
        item_title: "Fix bug".into(),
        status: "in-progress".into(),
        branch: Some("fix/bug".into()),
        pr: Some("https://github.com/org/repo/pull/1".into()),
        pr_ci_status: Some("success".into()),
        pr_comments: 3,
        unresolved_threads: 1,
        unreplied_threads: 0,
        unaddressed_issue_comments: 0,
        pr_body: "## Summary\nFixes bug".into(),
        changed_files: vec!["src/main.rs".into()],
        branch_ahead: true,
        process_alive: true,
        cpu_time_s: Some(120.5),
        prev_cpu_time_s: Some(60.0),
        stream_tail: "Building...".into(),
        seconds_active: 5400.0,
        intervention_count: 0,
        no_pr: false,
        reopen_seq: 0,
        has_reopen_ack: false,
        reopen_source: None,
        stream_stale_s: None,
        pr_head_sha: "abc123".into(),
        degraded: false,
        has_evidence: false,
        evidence_fresh: false,
        has_work_summary: false,
        work_summary_fresh: false,
    };
    assert_eq!(ctx.session_name, "worker-1-1");
    assert!(ctx.process_alive);
}

#[test]
fn construct_timeline_event() {
    let event = TimelineEvent {
        event_type: TimelineEventType::Created,
        timestamp: "2026-03-16T00:00:00Z".into(),
        actor: "human".into(),
        summary: "Item created".into(),
        data: serde_json::json!({"source": "telegram"}),
    };
    assert_eq!(event.event_type, TimelineEventType::Created);
}

#[test]
fn construct_ask_history_entry() {
    let entry = AskHistoryEntry {
        ask_id: "ask-001".into(),
        session_id: "sess-001".into(),
        role: "human".into(),
        content: "What does this do?".into(),
        timestamp: "2026-03-16T00:00:00Z".into(),
    };
    assert_eq!(entry.role, "human");
}

#[test]
fn construct_tick_result() {
    let mut tasks = HashMap::new();
    tasks.insert("in-progress".into(), 3);
    tasks.insert("queued".into(), 5);

    let result = TickResult {
        mode: TickMode::Live,
        tick_id: Some("abc12345".into()),
        max_workers: 10,
        active_workers: 3,
        tasks,
        alerts: vec!["Worker stale".into()],
        dry_actions: vec![Action {
            worker: "mando-worker-0".into(),
            action: ActionKind::Nudge,
            message: Some("Wake up".into()),
            reason: Some("Stale".into()),
        }],
        error: None,
        rate_limited: false,
    };
    assert_eq!(result.mode, TickMode::Live);
    assert_eq!(result.active_workers, 3);
}

#[test]
fn construct_notify_level() {
    let level = NotifyLevel::High;
    assert_eq!(level.value(), 30);
}

#[test]
fn construct_bus_event() {
    let event = BusEvent::Tasks;
    assert_eq!(event, BusEvent::Tasks);
}

#[test]
fn construct_session_entry() {
    let entry = SessionEntry {
        session_id: "abc-123".into(),
        ts: "2026-03-16T00:00:00Z".into(),
        cwd: "/tmp".into(),
        model: "opus-4".into(),
        caller: "worker".into(),
        resumed: false,
        source: "live".into(),
        cost_usd: Some(0.50),
        duration_ms: Some(30000),
        title: "Fix bug".into(),
        project: "acme/widgets".into(),
        task_id: "ENG-100".into(),
        worker_name: "mando-worker-0".into(),
        status: "done".into(),
    };
    assert_eq!(entry.session_id, "abc-123");
}

#[test]
fn item_status_has_15_values() {
    assert_eq!(crate::task::ALL_STATUSES.len(), 15);
}

#[test]
fn finalized_set() {
    assert!(FINALIZED.contains(&ItemStatus::Merged));
    assert!(FINALIZED.contains(&ItemStatus::CompletedNoPr));
    assert!(FINALIZED.contains(&ItemStatus::Canceled));
    assert!(!FINALIZED.contains(&ItemStatus::InProgress));
    assert_eq!(FINALIZED.len(), 3);
}

#[test]
fn reworkable_set() {
    assert!(REWORKABLE.contains(&ItemStatus::AwaitingReview));
    assert!(REWORKABLE.contains(&ItemStatus::Escalated));
    assert!(REWORKABLE.contains(&ItemStatus::HandedOff));
    assert!(REWORKABLE.contains(&ItemStatus::Errored));
    assert!(!REWORKABLE.contains(&ItemStatus::New));
    assert_eq!(REWORKABLE.len(), 4);
}

#[test]
fn reopenable_set() {
    assert!(REOPENABLE.contains(&ItemStatus::AwaitingReview));
    assert!(REOPENABLE.contains(&ItemStatus::Escalated));
    assert!(REOPENABLE.contains(&ItemStatus::HandedOff));
    assert!(REOPENABLE.contains(&ItemStatus::Errored));
    assert!(!REOPENABLE.contains(&ItemStatus::Queued));
    assert_eq!(REOPENABLE.len(), 4);
}

#[test]
fn status_method_helpers() {
    assert!(ItemStatus::Merged.is_finalized());
    assert!(!ItemStatus::InProgress.is_finalized());
}

#[test]
fn notify_level_ordering() {
    assert!(NotifyLevel::Low < NotifyLevel::Normal);
    assert!(NotifyLevel::Normal < NotifyLevel::High);
    assert!(NotifyLevel::High < NotifyLevel::Critical);
}

#[test]
fn notify_level_values() {
    assert_eq!(NotifyLevel::Low.value(), 10);
    assert_eq!(NotifyLevel::Normal.value(), 20);
    assert_eq!(NotifyLevel::High.value(), 30);
    assert_eq!(NotifyLevel::Critical.value(), 40);
}
