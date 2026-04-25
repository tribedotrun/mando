use super::*;

fn test_workflow() -> CaptainWorkflow {
    CaptainWorkflow::compiled_default()
}

async fn test_store_lock(_dir: &std::path::Path) -> Arc<RwLock<TaskStore>> {
    let db = global_db::Db::open_in_memory().await.unwrap();
    settings::projects::upsert(db.pool(), "test", "", None)
        .await
        .unwrap();
    let store = TaskStore::new(db.pool().clone());
    Arc::new(RwLock::new(store))
}

#[tokio::test]
async fn tick_no_tasks() {
    let dir = std::env::temp_dir().join("mando-tick-test-none");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store_lock = test_store_lock(&dir).await;
    let config = Config::default();
    let wf = test_workflow();
    let cancel = tokio_util::sync::CancellationToken::new();
    let result = run_captain_tick_inner(
        &config,
        &wf,
        true,
        None,
        true,
        &store_lock,
        &cancel,
        &tokio_util::task::TaskTracker::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.mode, TickMode::DryRun);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn tick_dry_run_does_not_mutate() {
    let dir = std::env::temp_dir().join("mando-tick-test-dry");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store_lock = test_store_lock(&dir).await;
    {
        let store = store_lock.write().await;
        let mut t = crate::Task::new("Test task");
        t.project_id = 1;
        t.project = "test".into();
        t.status = ItemStatus::New;
        store.add(t).await.unwrap();
    }
    let config = Config::default();
    let wf = test_workflow();
    let cancel = tokio_util::sync::CancellationToken::new();
    let result = run_captain_tick_inner(
        &config,
        &wf,
        true,
        None,
        true,
        &store_lock,
        &cancel,
        &tokio_util::task::TaskTracker::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.mode, TickMode::DryRun);
    assert!(result.error.is_none());
    assert_eq!(result.tasks.get("new"), Some(&1));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn tick_live_retries_clarifier_on_failure() {
    let dir = std::env::temp_dir().join("mando-tick-test-live");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store_lock = test_store_lock(&dir).await;

    // Build a config with one project whose path resolves under the temp
    // dir. resolve_clarifier_cwd now returns an error if the project is
    // unknown, so the test cannot rely on the previous silent fallback to
    // $HOME; the project must actually exist in the config.
    let project_path = dir.join("acme-widgets");
    std::fs::create_dir_all(&project_path).unwrap();
    let mut config = Config::default();
    config.captain.projects.insert(
        project_path.to_string_lossy().to_string(),
        settings::ProjectConfig {
            name: "acme/widgets".into(),
            aliases: vec![],
            path: project_path.to_string_lossy().to_string(),
            check_command: String::new(),
            worker_preamble: String::new(),
            scout_summary: String::new(),
            github_repo: None,
            logo: None,
            hooks: Default::default(),
            classify_rules: vec![],
        },
    );

    {
        let store = store_lock.write().await;
        let acme_id = settings::projects::upsert(store.pool(), "acme/widgets", "", None)
            .await
            .unwrap();
        let mut t = crate::Task::new("Lifecycle test item");
        t.project_id = acme_id;
        t.project = "acme/widgets".into();
        t.status = ItemStatus::New;
        store.add(t).await.unwrap();
    }
    let wf = test_workflow();

    // Hide the claude binary so the clarifier CC spawn fails. HOME stays
    // writable because the tick persists state (ops log, health) under
    // $HOME/.mando and those saves are now hard errors.
    let orig_path = std::env::var("PATH").unwrap_or_default();
    let orig_home = std::env::var("HOME").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent");
    std::env::set_var("HOME", dir.to_string_lossy().to_string());
    // Create the cc-streams dir so the async clarifier task can write
    // error results to stream files.
    std::fs::create_dir_all(dir.join(".mando/state/cc-streams")).unwrap();

    // Tick 1: dispatch spawns async clarifier task (fails quickly, writes
    // error to stream file). Task moves to Clarifying.
    let cancel = tokio_util::sync::CancellationToken::new();
    let result = run_captain_tick_inner(
        &config,
        &wf,
        false,
        None,
        true,
        &store_lock,
        &cancel,
        &tokio_util::task::TaskTracker::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.mode, TickMode::Live);
    assert!(result.error.is_none());
    // After tick 1, task is Clarifying (async session spawned).
    assert_eq!(result.tasks.get("clarifying"), Some(&1));

    // Wait for the async task to finish writing the error to the stream file.
    // Under full-suite load the background task can be delayed, so a fixed
    // sleep is flaky here.
    let clarifier_session_id = {
        let store = store_lock.read().await;
        store
            .find_by_id(1)
            .await
            .unwrap()
            .unwrap()
            .session_ids
            .clarifier
            .unwrap()
    };
    let stream_path = global_infra::paths::stream_path_for_session(&clarifier_session_id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ready = global_claude::get_stream_result(&stream_path)
            .and_then(|result| result.get("is_error").and_then(|value| value.as_bool()))
            == Some(true);
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for clarifier error result"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Tick 2: poll detects the error, reverts to New, increments fail count.
    let result2 = run_captain_tick_inner(
        &config,
        &wf,
        false,
        None,
        true,
        &store_lock,
        &cancel,
        &tokio_util::task::TaskTracker::new(),
    )
    .await
    .unwrap();

    std::env::set_var("PATH", &orig_path);
    std::env::set_var("HOME", &orig_home);

    assert_eq!(result2.mode, TickMode::Live);
    // The poll detected the failure and reverted to New, then the dispatch
    // phase in the same tick re-dispatched it to Clarifying (retry).
    assert_eq!(result2.tasks.get("clarifying"), Some(&1));

    // Verify clarifier failure count incremented (proves the poll processed the error).
    let store = store_lock.read().await;
    let task = store.find_by_id(1).await.unwrap().unwrap();
    assert_eq!(task.clarifier_fail_count, 1);
    std::fs::remove_dir_all(&dir).ok();
}
