use super::*;

#[path = "captain_review_schema_tests.rs"]
mod schema;

#[path = "captain_review_render_tests.rs"]
mod render;

#[path = "captain_review_validate_tests.rs"]
mod validate;

#[test]
fn test_check_review_parses_structured_output() {
    use std::io::Write;

    let session_id = "test-check-review-structured";
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

    let mut f = std::fs::File::create(&stream_path).unwrap();
    writeln!(
        f,
        r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
    )
    .unwrap();
    writeln!(
            f,
            r#"{{"type":"result","subtype":"success","result":"","structured_output":{{"action":"ship","feedback":"looks good"}}}}"#
        )
        .unwrap();

    let item = Task {
        session_ids: crate::SessionIds {
            review: Some(session_id.to_string()),
            ..Default::default()
        },
        captain_review_trigger: Some(crate::ReviewTrigger::GatesPass),
        ..Task::new("test")
    };

    let verdict = check_review(&item).unwrap();
    assert_eq!(verdict.action, "ship");
    assert_eq!(verdict.feedback, "looks good");

    std::fs::remove_file(&stream_path).ok();
}

#[test]
fn test_check_review_falls_back_to_assistant_text() {
    use std::io::Write;

    let session_id = "test-check-review-fallback";
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

    let mut f = std::fs::File::create(&stream_path).unwrap();
    writeln!(
        f,
        r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
    )
    .unwrap();
    writeln!(
            f,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{{\"action\":\"nudge\",\"feedback\":\"add tests\"}}"}}]}}}}"#
        )
        .unwrap();
    writeln!(
        f,
        r#"{{"type":"result","subtype":"success","result":"","structured_output":null}}"#
    )
    .unwrap();

    let item = Task {
        session_ids: crate::SessionIds {
            review: Some(session_id.to_string()),
            ..Default::default()
        },
        captain_review_trigger: Some(crate::ReviewTrigger::GatesPass),
        ..Task::new("test")
    };

    let verdict = check_review(&item).unwrap();
    assert_eq!(verdict.action, "nudge");
    assert_eq!(verdict.feedback, "add tests");

    std::fs::remove_file(&stream_path).ok();
}

#[test]
fn test_check_review_escalates_when_all_paths_empty() {
    use std::io::Write;

    let session_id = "test-check-review-all-empty";
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

    // Session completed but no structured_output, no result text, no assistant text.
    let mut f = std::fs::File::create(&stream_path).unwrap();
    writeln!(
        f,
        r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"result","subtype":"success","result":"","structured_output":null}}"#
    )
    .unwrap();

    let item = Task {
        session_ids: crate::SessionIds {
            review: Some(session_id.to_string()),
            ..Default::default()
        },
        captain_review_trigger: Some(crate::ReviewTrigger::GatesPass),
        ..Task::new("test")
    };

    let verdict = check_review(&item).unwrap();
    assert_eq!(verdict.action, "escalate");
    assert!(verdict.feedback.contains("no extractable verdict"));
    assert!(
        verdict.report.is_some(),
        "escalation must have a CTO report"
    );

    std::fs::remove_file(&stream_path).ok();
}

#[test]
fn test_reset_review_retry_starts_fresh_cycle() {
    let mut item = Task::new("test");
    item.status = crate::ItemStatus::Errored;
    item.review_fail_count = 4;
    item.session_ids.review = Some("old-review".into());

    crate::runtime::action_contract::reset_review_retry(&mut item, crate::ReviewTrigger::Retry);

    assert_eq!(item.status, crate::ItemStatus::CaptainReviewing);
    assert_eq!(item.review_fail_count, 0);
    assert!(item.session_ids.review.is_none());
    assert_eq!(
        item.captain_review_trigger,
        Some(crate::ReviewTrigger::Retry)
    );
    assert!(item.last_activity_at.is_some());
}

#[tokio::test]
async fn test_spawn_review_preserves_existing_review_fail_count() {
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let workflow = settings::CaptainWorkflow::compiled_default();

    let worktree =
        std::env::temp_dir().join(format!("mando-captain-review-test-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).unwrap();

    // Seed a test project so FK constraints are satisfied.
    let project_id = settings::projects::upsert(&pool, "test", "", None)
        .await
        .unwrap();
    let wb_id = crate::io::test_support::seed_workbench(&pool, project_id).await;

    let mut item = Task::new("test");
    item.project_id = project_id;
    item.project = "test".into();
    item.workbench_id = wb_id;
    item.status = crate::ItemStatus::CaptainReviewing;
    item.review_fail_count = 4;
    item.worktree = Some(worktree.to_string_lossy().to_string());

    // Insert the task into the DB so persist_status_transition's guard can match.
    let store = crate::io::task_store::TaskStore::new(pool.clone());
    let id = store.add(item.clone()).await.unwrap();
    item.id = id;
    // Re-set status after insert (add() may normalize it).
    store
        .update(id, |t| {
            t.status = crate::ItemStatus::CaptainReviewing;
            t.review_fail_count = 4;
        })
        .await
        .unwrap();
    item.status = crate::ItemStatus::CaptainReviewing;

    spawn_review(
        &mut item,
        "retry",
        None, // already CaptainReviewing in DB
        &settings::Config::default(),
        &workflow,
        &notifier,
        &pool,
    )
    .await
    .unwrap();

    assert_eq!(item.review_fail_count, 4);
    assert!(item.session_ids.review.is_some());
}

#[tokio::test]
async fn test_review_failure_budget_moves_item_to_errored_on_fifth_attempt() {
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let workflow = settings::CaptainWorkflow::compiled_default();

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.review_fail_count = 4;
    item.captain_review_trigger = Some(crate::ReviewTrigger::Retry);
    item.session_ids.review = Some("review-session".into());

    handle_review_error(
        &mut item,
        "review session timed out without producing a verdict",
        &workflow,
        &notifier,
        &pool,
    )
    .await;

    assert_eq!(item.review_fail_count, 5);
    assert_eq!(item.status, crate::ItemStatus::Errored);
    assert!(item.session_ids.review.is_none());
    assert!(item.captain_review_trigger.is_none());
}

#[tokio::test]
async fn test_nudge_verdict_resets_worker_started_at() {
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.worker_started_at = Some("2020-01-01T00:00:00Z".to_string());

    let verdict = CaptainVerdict {
        action: "nudge".into(),
        feedback: "keep going".into(),
        ..Default::default()
    };
    let config = settings::Config::default();
    let workflow = settings::CaptainWorkflow::compiled_default();
    apply_verdict(&mut item, &verdict, &config, &workflow, &notifier, &pool)
        .await
        .unwrap();

    assert_eq!(item.status, crate::ItemStatus::InProgress);
    // worker_started_at must be reset to ~now, not the old 2020 timestamp.
    let started = item.worker_started_at.as_deref().unwrap();
    assert_ne!(started, "2020-01-01T00:00:00Z", "timestamp was not reset");
    // Verify it's a valid RFC 3339 timestamp within the last 5 seconds.
    let parsed =
        time::OffsetDateTime::parse(started, &time::format_description::well_known::Rfc3339)
            .expect("worker_started_at must be valid RFC 3339");
    let elapsed = (time::OffsetDateTime::now_utc() - parsed).as_seconds_f64();
    assert!(
        elapsed < 5.0,
        "expected timestamp within last 5s, got {elapsed}s ago"
    );
}

#[tokio::test]
async fn test_apply_verdict_nudge_preserves_review_context_on_failed_resume() {
    // When nudge resume fails (no worker/session/worktree to resume), the
    // review context must be preserved so the next tick can retry. Previously
    // this was silently cleared, causing the task to lose its review trigger
    // and review_fail_count, which left it stuck in InProgress without any
    // review context on the next tick.
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.review_fail_count = 4;
    item.session_ids.review = Some("review-session".into());

    let verdict = CaptainVerdict {
        action: "nudge".into(),
        feedback: "try again".into(),
        ..Default::default()
    };
    let config = settings::Config::default();
    let workflow = settings::CaptainWorkflow::compiled_default();
    apply_verdict(&mut item, &verdict, &config, &workflow, &notifier, &pool)
        .await
        .unwrap();

    // Status still transitions so the UI reflects the nudge attempt.
    assert_eq!(item.status, crate::ItemStatus::InProgress);
    // On failed resume, review fields are preserved for the next tick's retry.
    assert_eq!(item.review_fail_count, 4);
    assert!(item.session_ids.review.is_some());
}

#[tokio::test]
async fn test_reset_budget_verdict_resets_intervention_count() {
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.intervention_count = 42;
    item.worker_started_at = Some("2020-01-01T00:00:00Z".to_string());

    let verdict = CaptainVerdict {
        action: "reset_budget".into(),
        feedback: "try a different approach".into(),
        ..Default::default()
    };
    let config = settings::Config::default();
    let workflow = settings::CaptainWorkflow::compiled_default();
    apply_verdict(&mut item, &verdict, &config, &workflow, &notifier, &pool)
        .await
        .unwrap();

    assert_eq!(item.status, crate::ItemStatus::InProgress);
    assert_eq!(item.intervention_count, 0, "budget must be reset to 0");
    // worker_started_at must be reset to ~now.
    let started = item.worker_started_at.as_deref().unwrap();
    assert_ne!(started, "2020-01-01T00:00:00Z", "timestamp was not reset");
}

#[tokio::test]
async fn test_respawn_verdict_preserves_worktree() {
    // Respawn must reuse the existing worktree. Captain invariant #4 in
    // CLAUDE.md: "Rework = same worktree with new branch + worker". The
    // spawner's 3-way match in spawner.rs picks the Rework arm iff
    // item.worktree is still Some(..) and item.branch is None; clearing
    // worktree here forces the fallthrough arm which allocates a new slot
    // from worker-counter.txt and creates a brand-new dir on disk.
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.worker = Some("worker-1-1".into());
    item.worktree = Some("/tmp/mando-todo-1".into());
    item.branch = Some("mando/todo-1".into());
    item.pr_number = Some(100);
    item.session_ids.worker = Some("worker-sid".into());
    item.worker_started_at = Some("2020-01-01T00:00:00Z".to_string());

    let verdict = CaptainVerdict {
        action: "respawn".into(),
        feedback: "session is broken, retry".into(),
        ..Default::default()
    };
    let config = settings::Config::default();
    let workflow = settings::CaptainWorkflow::compiled_default();
    apply_verdict(&mut item, &verdict, &config, &workflow, &notifier, &pool)
        .await
        .unwrap();

    assert_eq!(
        item.worktree.as_deref(),
        Some("/tmp/mando-todo-1"),
        "respawn must preserve item.worktree so the next spawn hits the \
         spawner's Rework arm (same worktree, new branch from origin/main)",
    );
    assert!(item.worker.is_none(), "worker cleared");
    assert!(item.branch.is_none(), "branch cleared");
    assert!(
        item.session_ids.worker.is_none(),
        "worker session id cleared"
    );
    assert_eq!(item.pr_number, None, "pr_number cleared");
}

#[tokio::test]
async fn test_reset_budget_preserves_review_fields_on_failed_resume() {
    let db = global_db::Db::open_in_memory().await.unwrap();
    let pool = db.pool().clone();
    let notifier =
        crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));

    let mut item = Task::new("test");
    item.status = crate::ItemStatus::CaptainReviewing;
    item.intervention_count = 50;
    item.review_fail_count = 2;
    item.session_ids.review = Some("review-session".into());

    let verdict = CaptainVerdict {
        action: "reset_budget".into(),
        feedback: "unblock this".into(),
        ..Default::default()
    };
    let config = settings::Config::default();
    let workflow = settings::CaptainWorkflow::compiled_default();
    apply_verdict(&mut item, &verdict, &config, &workflow, &notifier, &pool)
        .await
        .unwrap();

    assert_eq!(item.status, crate::ItemStatus::InProgress);
    assert_eq!(item.intervention_count, 0);
    assert_eq!(item.review_fail_count, 2);
    assert!(item.session_ids.review.is_some());
}
