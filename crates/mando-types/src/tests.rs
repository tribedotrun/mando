//! Tests for mando-types.

use std::collections::HashMap;

use crate::task::{FINALIZED, REOPENABLE, REWORKABLE};
use crate::{
    Action, ActionKind, AskHistoryEntry, BusEvent, CronJob, CronPayload, CronSchedule, CronState,
    ItemStatus, NotifyLevel, ScoutItem, ScoutStatus, SessionEntry, Task, TickResult, TimelineEvent,
    TimelineEventType, WorkerContext,
};

// -----------------------------------------------------------------------
// Construction tests — every type can be instantiated
// -----------------------------------------------------------------------

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
        ItemStatus::AwaitingReview,
        ItemStatus::Rework,
        ItemStatus::HandedOff,
        ItemStatus::Escalated,
        ItemStatus::Errored,
        ItemStatus::Merged,
        ItemStatus::CompletedNoPr,
        ItemStatus::Canceled,
    ];
    assert_eq!(statuses.len(), 14);
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
    };
    assert_eq!(item.id, 42);
    assert_eq!(item.status, ScoutStatus::Pending);
}

#[test]
fn construct_cron_job() {
    let job = CronJob {
        id: "cron-1".into(),
        name: "daily-scout".into(),
        enabled: true,
        schedule: CronSchedule {
            kind: "cron".into(),
            expr: Some("0 9 * * *".into()),
            ..CronSchedule::default()
        },
        payload: CronPayload::default(),
        state: CronState::default(),
        created_at_ms: 1000,
        updated_at_ms: 2000,
        delete_after_run: false,
        job_type: "system".into(),
        cwd: None,
        timeout_s: 1200,
    };
    assert_eq!(job.id, "cron-1");
    assert!(job.enabled);
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
        mode: "live".into(),
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
    };
    assert_eq!(result.mode, "live");
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

// -----------------------------------------------------------------------
// ItemStatus — status group membership
// -----------------------------------------------------------------------

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
    assert!(ItemStatus::AwaitingReview.is_reworkable());
    assert!(ItemStatus::Errored.is_reopenable());
}

// -----------------------------------------------------------------------
// NotifyLevel — integer ordering
// -----------------------------------------------------------------------

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

// -----------------------------------------------------------------------
// Serde round-trip tests
// -----------------------------------------------------------------------

#[test]
fn serde_item_status_kebab_case() {
    assert_eq!(
        serde_json::to_string(&ItemStatus::InProgress).unwrap(),
        "\"in-progress\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::Escalated).unwrap(),
        "\"escalated\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::AwaitingReview).unwrap(),
        "\"awaiting-review\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::HandedOff).unwrap(),
        "\"handed-off\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::CompletedNoPr).unwrap(),
        "\"completed-no-pr\""
    );
    assert_eq!(serde_json::to_string(&ItemStatus::New).unwrap(), "\"new\"");
    assert_eq!(
        serde_json::to_string(&ItemStatus::CaptainReviewing).unwrap(),
        "\"captain-reviewing\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::Errored).unwrap(),
        "\"errored\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::NeedsClarification).unwrap(),
        "\"needs-clarification\""
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::Queued).unwrap(),
        "\"queued\""
    );
}

#[test]
fn serde_item_status_round_trip() {
    for status in crate::task::ALL_STATUSES {
        let json = serde_json::to_string(&status).unwrap();
        let parsed: ItemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, parsed);
    }
}

#[test]
fn serde_task_round_trip() {
    let mut item = Task::new("Test item");
    item.status = ItemStatus::InProgress;
    item.project = Some("acme/widgets".into());
    item.intervention_count = 5;

    let json = serde_json::to_string(&item).unwrap();
    let parsed: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.title, "Test item");
    assert_eq!(parsed.status, ItemStatus::InProgress);
    assert_eq!(parsed.project, Some("acme/widgets".into()));
    assert_eq!(parsed.intervention_count, 5);
    assert!(!parsed.status.is_finalized());
}

#[test]
fn serde_cron_job_round_trip() {
    let job = CronJob {
        id: "cron-42".into(),
        name: "morning-report".into(),
        enabled: true,
        schedule: CronSchedule {
            kind: "cron".into(),
            expr: Some("0 9 * * *".into()),
            tz: Some("America/New_York".into()),
            ..CronSchedule::default()
        },
        payload: CronPayload {
            kind: "agent_turn".into(),
            message: "Good morning".into(),
            deliver: true,
            channel: Some("telegram".into()),
            to: None,
        },
        state: CronState {
            next_run_at_ms: Some(1710000000000),
            last_run_at_ms: None,
            last_status: Some("ok".into()),
            last_error: None,
        },
        created_at_ms: 1000,
        updated_at_ms: 2000,
        delete_after_run: false,
        job_type: "system".into(),
        cwd: None,
        timeout_s: 600,
    };

    let json = serde_json::to_string(&job).unwrap();
    let parsed: CronJob = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, "cron-42");
    assert_eq!(parsed.name, "morning-report");
    assert!(parsed.enabled);
    assert_eq!(parsed.schedule.expr, Some("0 9 * * *".into()));
    assert_eq!(parsed.state.last_status, Some("ok".into()));
    assert_eq!(parsed.timeout_s, 600);
}

#[test]
fn serde_bus_event_lowercase() {
    assert_eq!(
        serde_json::to_string(&BusEvent::Tasks).unwrap(),
        "\"tasks\""
    );
    assert_eq!(
        serde_json::to_string(&BusEvent::Scout).unwrap(),
        "\"scout\""
    );
    assert_eq!(
        serde_json::to_string(&BusEvent::Sessions).unwrap(),
        "\"sessions\""
    );

    let parsed: BusEvent = serde_json::from_str("\"status\"").unwrap();
    assert_eq!(parsed, BusEvent::Status);
}

#[test]
fn serde_action_kind_kebab() {
    assert_eq!(
        serde_json::to_string(&ActionKind::Ship).unwrap(),
        "\"ship\""
    );
    assert_eq!(
        serde_json::to_string(&ActionKind::CaptainReview).unwrap(),
        "\"captain-review\""
    );
    assert_eq!(
        serde_json::to_string(&ActionKind::Skip).unwrap(),
        "\"skip\""
    );
}

#[test]
fn serde_timeline_event_type_round_trip() {
    let types = [
        TimelineEventType::Created,
        TimelineEventType::ClarifyStarted,
        TimelineEventType::WorkerSpawned,
        TimelineEventType::AwaitingReview,
        TimelineEventType::Escalated,
        TimelineEventType::CaptainReviewStarted,
        TimelineEventType::CaptainReviewVerdict,
    ];
    for t in types {
        let json = serde_json::to_string(&t).unwrap();
        let parsed: TimelineEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(t, parsed);
    }
}

#[test]
fn serde_scout_status_round_trip() {
    let statuses = [
        ScoutStatus::Pending,
        ScoutStatus::Fetched,
        ScoutStatus::Processed,
        ScoutStatus::Saved,
        ScoutStatus::Archived,
        ScoutStatus::Error,
    ];
    for s in statuses {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: ScoutStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(s, parsed);
    }
}

#[test]
fn serde_notify_level_round_trip() {
    let levels = [
        NotifyLevel::Low,
        NotifyLevel::Normal,
        NotifyLevel::High,
        NotifyLevel::Critical,
    ];
    for l in levels {
        let json = serde_json::to_string(&l).unwrap();
        let parsed: NotifyLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(l, parsed);
    }
}
