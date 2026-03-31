use super::*;

fn test_workflow() -> CaptainWorkflow {
    CaptainWorkflow::compiled_default()
}

async fn test_store_lock(_dir: &std::path::Path) -> Arc<RwLock<TaskStore>> {
    let db = mando_db::Db::open_in_memory().await.unwrap();
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
    let result = run_captain_tick_inner(&config, &wf, true, None, true, &store_lock)
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
        let mut t = mando_types::Task::new("Test task");
        t.status = ItemStatus::New;
        store.add(t).await.unwrap();
    }
    let config = Config::default();
    let wf = test_workflow();
    let result = run_captain_tick_inner(&config, &wf, true, None, true, &store_lock)
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
    {
        let store = store_lock.write().await;
        let mut t = mando_types::Task::new("Lifecycle test item");
        t.status = ItemStatus::New;
        t.project = Some("acme/widgets".into());
        store.add(t).await.unwrap();
    }
    let config = Config::default();
    let wf = test_workflow();

    // Hide the claude binary so the clarifier fails.
    let orig_path = std::env::var("PATH").unwrap_or_default();
    let orig_home = std::env::var("HOME").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent");
    std::env::set_var("HOME", "/nonexistent");

    let result = run_captain_tick_inner(&config, &wf, false, None, true, &store_lock)
        .await
        .unwrap();

    std::env::set_var("PATH", &orig_path);
    std::env::set_var("HOME", &orig_home);

    assert_eq!(result.mode, TickMode::Live);
    assert!(result.error.is_none());
    // Task stays New (retryable), not auto-promoted to Ready.
    assert_eq!(result.tasks.get("new"), Some(&1));
    assert_eq!(result.tasks.get("ready"), None);

    // Verify clarifier failure count incremented.
    let store = store_lock.read().await;
    let task = store.find_by_id(1).await.unwrap().unwrap();
    assert_eq!(task.clarifier_fail_count, 1);
    std::fs::remove_dir_all(&dir).ok();
}
