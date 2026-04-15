//! Integration tests for `poll_triage` and `emit_exhaustion`.
//!
//! Each test uses:
//! - An in-memory SQLite pool
//! - A temp `MANDO_DATA_DIR` so `stream_path_for_session` routes to an
//!   isolated directory that the test can write fake CC stream files into
//! - A real `Notifier` with a fresh `EventBus`
//!
//! These tests prove the end-to-end: CC stream content → classify →
//! timeline event + notifier message + session cleared.

use std::io::Write;
use std::path::PathBuf;

use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::auto_merge_triage::{emit_exhaustion, poll_triage};
use super::notify::Notifier;

fn isolate_data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mando-triage-test-{tag}-{}",
        mando_uuid::Uuid::v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: tests within this module mutate the process-wide
    // `MANDO_DATA_DIR`. Each call uses a UUID-suffixed directory, and the
    // tests do not assert against the env var across awaits — they read it
    // only via `data_dir()` synchronously after setting it, before any
    // await point. Other test modules in this crate (e.g.
    // `mergeability_rebase::tests::isolate_data_dir`) follow the same
    // pattern. Required because Rust 1.81+ marks `set_var` as unsafe.
    unsafe { std::env::set_var("MANDO_DATA_DIR", &dir) };
    dir
}

async fn test_pool() -> sqlx::SqlitePool {
    let db = mando_db::Db::open_in_memory().await.unwrap();
    db.pool().clone()
}

fn write_stream_file(data_dir: &std::path::Path, session_id: &str, result_line: &str) {
    let streams_dir = data_dir.join("state/cc-streams");
    std::fs::create_dir_all(&streams_dir).unwrap();
    let stream_path = streams_dir.join(format!("{session_id}.jsonl"));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stream_path)
        .unwrap();
    // System init event so `current_session_lines` has a session boundary.
    writeln!(
        f,
        r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
    )
    .unwrap();
    writeln!(f, "{result_line}").unwrap();
}

async fn seed_task_with_triage_session(pool: &sqlx::SqlitePool, session_id: &str) -> Task {
    mando_db::queries::projects::upsert(pool, "test", "", None)
        .await
        .unwrap();
    let mut item = Task::new("triage test");
    item.project_id = 1;
    item.project = "test".into();
    item.status = ItemStatus::AwaitingReview;
    item.pr_number = Some(100);
    item.session_ids.triage = Some(session_id.to_string());
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let store = crate::io::task_store::TaskStore::new(pool.clone());
    let id = store.add(item.clone()).await.unwrap();
    item.id = id;
    // Re-assert fields that `add()` may normalize.
    store
        .update(id, |t| {
            t.status = ItemStatus::AwaitingReview;
            t.pr_number = Some(100);
            t.session_ids.triage = Some(session_id.to_string());
        })
        .await
        .unwrap();
    item
}

fn default_notifier() -> Notifier {
    Notifier::new(std::sync::Arc::new(mando_shared::EventBus::new()))
}

#[tokio::test]
async fn poll_triage_emits_failed_on_is_error_without_synthetic_verdict() {
    let data_dir = isolate_data_dir("is-error");
    let pool = test_pool().await;
    let session_id = mando_uuid::Uuid::v4().to_string();

    // CC errored out (e.g. stream idle timeout — matches what CC emits itself,
    // with text in the `result` field, NOT the `error` field).
    write_stream_file(
        &data_dir,
        &session_id,
        r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: Stream idle timeout - partial response received"}"#,
    );

    let item = seed_task_with_triage_session(&pool, &session_id).await;
    let mut items = vec![item];

    let notifier = default_notifier();
    let config = mando_config::Config::default();
    let workflow = mando_config::workflow::CaptainWorkflow::compiled_default();

    poll_triage(&mut items, &config, &workflow, &notifier, &pool).await;

    // Session cleared so the next tick can re-spawn.
    assert!(items[0].session_ids.triage.is_none());

    // Timeline: AutoMergeTriageFailed carries the real error text (not
    // "unknown error" as the old code wrote) AND no AutoMergeTriage verdict.
    let events = mando_db::queries::timeline::load(&pool, items[0].id)
        .await
        .unwrap();
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriageFailed)
        .collect();
    let verdicts: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriage)
        .collect();
    assert_eq!(failed.len(), 1, "expected exactly one failed event");
    assert_eq!(verdicts.len(), 0, "no synthetic verdict should be emitted");
    let error_text = failed[0]
        .data
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(
        error_text.contains("Stream idle timeout"),
        "expected real CC error text, got {error_text:?}"
    );
    let attempt = failed[0]
        .data
        .get("attempt")
        .and_then(|v| v.as_u64())
        .unwrap();
    assert_eq!(attempt, 1);
}

#[tokio::test]
async fn poll_triage_emits_verdict_on_structured_success() {
    let data_dir = isolate_data_dir("verdict");
    let pool = test_pool().await;
    let session_id = mando_uuid::Uuid::v4().to_string();

    write_stream_file(
        &data_dir,
        &session_id,
        r#"{"type":"result","subtype":"success","is_error":false,"structured_output":{"confidence":"mid","reason":"Large PR — human review preferred."}}"#,
    );

    let item = seed_task_with_triage_session(&pool, &session_id).await;
    let mut items = vec![item];

    let notifier = default_notifier();
    let config = mando_config::Config::default();
    let workflow = mando_config::workflow::CaptainWorkflow::compiled_default();

    poll_triage(&mut items, &config, &workflow, &notifier, &pool).await;

    assert!(items[0].session_ids.triage.is_none());

    let events = mando_db::queries::timeline::load(&pool, items[0].id)
        .await
        .unwrap();
    let verdicts: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriage)
        .collect();
    assert_eq!(verdicts.len(), 1, "expected exactly one verdict event");
    assert_eq!(
        verdicts[0].data.get("confidence").and_then(|v| v.as_str()),
        Some("mid")
    );
    assert_eq!(
        verdicts[0].data.get("reason").and_then(|v| v.as_str()),
        Some("Large PR — human review preferred.")
    );
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriageFailed)
        .collect();
    assert_eq!(failed.len(), 0);
}

#[tokio::test]
async fn poll_triage_three_consecutive_failures_increment_attempt_number() {
    let data_dir = isolate_data_dir("three-failures");
    let pool = test_pool().await;

    let config = mando_config::Config::default();
    let workflow = mando_config::workflow::CaptainWorkflow::compiled_default();
    let notifier = default_notifier();

    // First attempt: new task + fake CC error.
    let session1 = mando_uuid::Uuid::v4().to_string();
    write_stream_file(
        &data_dir,
        &session1,
        r#"{"type":"result","subtype":"error","is_error":true,"error":"boom-1"}"#,
    );
    let mut item = seed_task_with_triage_session(&pool, &session1).await;
    let mut items = vec![item.clone()];
    poll_triage(&mut items, &config, &workflow, &notifier, &pool).await;
    item = items[0].clone();
    assert!(item.session_ids.triage.is_none());

    // Second attempt: simulate a new spawn with a fresh session_id.
    let session2 = mando_uuid::Uuid::v4().to_string();
    write_stream_file(
        &data_dir,
        &session2,
        r#"{"type":"result","subtype":"error","is_error":true,"error":"boom-2"}"#,
    );
    item.session_ids.triage = Some(session2.clone());
    let mut items = vec![item.clone()];
    poll_triage(&mut items, &config, &workflow, &notifier, &pool).await;
    item = items[0].clone();
    assert!(item.session_ids.triage.is_none());

    // Third attempt.
    let session3 = mando_uuid::Uuid::v4().to_string();
    write_stream_file(
        &data_dir,
        &session3,
        r#"{"type":"result","subtype":"error","is_error":true,"error":"boom-3"}"#,
    );
    item.session_ids.triage = Some(session3.clone());
    let mut items = vec![item.clone()];
    poll_triage(&mut items, &config, &workflow, &notifier, &pool).await;

    let events = mando_db::queries::timeline::load(&pool, item.id)
        .await
        .unwrap();
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriageFailed)
        .collect();
    assert_eq!(failed.len(), 3);
    let attempts: Vec<u64> = failed
        .iter()
        .map(|e| e.data.get("attempt").and_then(|v| v.as_u64()).unwrap())
        .collect();
    assert_eq!(attempts, vec![1, 2, 3]);
    let errors: Vec<&str> = failed
        .iter()
        .map(|e| e.data.get("error").and_then(|v| v.as_str()).unwrap())
        .collect();
    assert_eq!(errors, vec!["boom-1", "boom-2", "boom-3"]);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == TimelineEventType::AutoMergeTriage)
            .count(),
        0,
        "no synthetic verdict should ever be emitted"
    );
}

#[tokio::test]
async fn emit_exhaustion_writes_terminal_event_with_last_error() {
    let _data_dir = isolate_data_dir("exhaustion");
    let pool = test_pool().await;
    let notifier = default_notifier();

    mando_db::queries::projects::upsert(&pool, "test", "", None)
        .await
        .unwrap();
    let mut item = Task::new("exhaust");
    item.project_id = 1;
    item.project = "test".into();
    item.status = ItemStatus::AwaitingReview;
    item.pr_number = Some(200);
    let store = crate::io::task_store::TaskStore::new(pool.clone());
    let id = store.add(item.clone()).await.unwrap();
    item.id = id;

    emit_exhaustion(&item, Some("Stream idle timeout"), 3, &notifier, &pool).await;

    let events = mando_db::queries::timeline::load(&pool, item.id)
        .await
        .unwrap();
    let ex: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == TimelineEventType::AutoMergeTriageExhausted)
        .collect();
    assert_eq!(ex.len(), 1);
    assert_eq!(ex[0].data.get("attempts").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(
        ex[0].data.get("last_error").and_then(|v| v.as_str()),
        Some("Stream idle timeout")
    );
}
