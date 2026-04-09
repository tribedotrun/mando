//! Serde round-trip and type-contract tests.

use crate::{ActionKind, BusEvent, ItemStatus, NotifyLevel, ScoutStatus, Task, TimelineEventType};

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
    item.project = "acme/widgets".into();
    item.intervention_count = 5;

    let json = serde_json::to_string(&item).unwrap();
    let parsed: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.title, "Test item");
    assert_eq!(parsed.status, ItemStatus::InProgress);
    assert_eq!(parsed.project, "acme/widgets");
    assert_eq!(parsed.intervention_count, 5);
    assert!(!parsed.status.is_finalized());
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
fn serde_scout_processed_notification_round_trip() {
    use crate::events::{NotificationKind, NotificationPayload};
    let payload = NotificationPayload {
        message: "Test".into(),
        level: NotifyLevel::Normal,
        kind: NotificationKind::ScoutProcessed {
            scout_id: 42,
            title: "Test".into(),
            relevance: 80,
            quality: 90,
            source_name: Some("Blog".into()),
            telegraph_url: Some("https://telegra.ph/t".into()),
        },
        task_key: Some("scout:42".into()),
        reply_markup: None,
    };
    let json = serde_json::to_string(&payload).unwrap();
    let parsed: NotificationPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.message, payload.message);
    assert!(matches!(
        &parsed.kind,
        NotificationKind::ScoutProcessed {
            scout_id: 42,
            relevance: 80,
            quality: 90,
            ..
        }
    ));
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

// -----------------------------------------------------------------------
// Type contract -- Rust serialization must match Electron types.ts.
// -----------------------------------------------------------------------

const TASK_API_FIELDS: &[&str] = &[
    "archived_at",
    "branch",
    "captain_review_trigger",
    "clarifier_fail_count",
    "context",
    "created_at",
    "escalation_report",
    "github_repo",
    "id",
    "images",
    "intervention_count",
    "last_activity_at",
    "merge_fail_count",
    "no_pr",
    "original_prompt",
    "plan",
    "pr_number",
    "project",
    "project_id",
    "reopen_seq",
    "reopen_source",
    "resource",
    "rev",
    "review_fail_count",
    "session_ids",
    "source",
    "spawn_fail_count",
    "status",
    "title",
    "workbench_id",
    "worker",
    "worker_seq",
    "worker_started_at",
    "worktree",
];

const SESSION_IDS_API_FIELDS: &[&str] = &["ask", "clarifier", "merge", "review", "worker"];

const ITEM_STATUS_API_VALUES: &[&str] = &[
    "awaiting-review",
    "canceled",
    "captain-merging",
    "captain-reviewing",
    "clarifying",
    "completed-no-pr",
    "errored",
    "escalated",
    "handed-off",
    "in-progress",
    "merged",
    "needs-clarification",
    "new",
    "queued",
    "rework",
];

#[test]
fn type_contract_task_fields() {
    let mut item = Task::new("contract");
    item.id = 1;
    item.project_id = 1;
    item.project = "p".into();
    item.worker = Some("w".into());
    item.resource = Some("r".into());
    item.context = Some("c".into());
    item.original_prompt = Some("o".into());
    item.created_at = Some("t".into());
    item.workbench_id = Some(1);
    item.worktree = Some("wt".into());
    item.branch = Some("b".into());
    item.pr_number = Some(1);
    item.worker_started_at = Some("t".into());
    item.captain_review_trigger = Some(crate::task::ReviewTrigger::GatesPass);
    item.session_ids = crate::SessionIds {
        worker: Some("s".into()),
        review: Some("s".into()),
        clarifier: Some("s".into()),
        merge: Some("s".into()),
        ask: Some("s".into()),
    };
    item.last_activity_at = Some("t".into());
    item.plan = Some("p".into());
    item.no_pr = true;
    item.worker_seq = 1;
    item.reopen_seq = 1;
    item.reopen_source = Some("r".into());
    item.images = Some("i".into());
    item.escalation_report = Some("e".into());
    item.source = Some("s".into());
    item.archived_at = Some("a".into());
    item.github_repo = Some("g".into());

    let json: serde_json::Value = serde_json::to_value(&item).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort();

    assert_eq!(
        keys, TASK_API_FIELDS,
        "Task JSON fields diverged from contract. \
         Update TASK_API_FIELDS and electron/src/renderer/types.ts"
    );
}

#[test]
fn type_contract_session_ids_fields() {
    let ids = crate::SessionIds {
        worker: Some("s".into()),
        review: Some("s".into()),
        clarifier: Some("s".into()),
        merge: Some("s".into()),
        ask: Some("s".into()),
    };
    let json: serde_json::Value = serde_json::to_value(&ids).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort();

    assert_eq!(
        keys, SESSION_IDS_API_FIELDS,
        "SessionIds JSON fields diverged from contract. \
         Update SESSION_IDS_API_FIELDS and electron/src/renderer/types.ts"
    );
}

#[test]
fn type_contract_item_status_values() {
    let mut values: Vec<&str> = crate::task::ALL_STATUSES
        .iter()
        .map(|s| s.as_str())
        .collect();
    values.sort();

    assert_eq!(
        values, ITEM_STATUS_API_VALUES,
        "ItemStatus variants diverged from contract. \
         Update ITEM_STATUS_API_VALUES and electron/src/renderer/types.ts"
    );
}
