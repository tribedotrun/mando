use super::*;

async fn test_pool() -> sqlx::SqlitePool {
    let db = global_db::Db::open_in_memory().await.unwrap();
    db.pool().clone()
}

#[tokio::test]
async fn dispatch_dry_run() {
    let pool = test_pool().await;
    let mut item = Task::new("Test dispatch");
    item.status = ItemStatus::Queued;
    item.id = 1;

    let config = Config::default();
    let workflow = CaptainWorkflow::compiled_default();
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![item];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();

    let result = dispatch_new_work(
        &mut items,
        &config,
        0,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &HashMap::new(),
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    assert_eq!(result, 1);
    assert_eq!(dry.len(), 1);
    assert!(dry[0].contains("spawn"));
}

#[tokio::test]
async fn dispatch_full_slots() {
    let pool = test_pool().await;
    let mut item = Task::new("Blocked");
    item.status = ItemStatus::Queued;

    let config = Config::default();
    let workflow = CaptainWorkflow::compiled_default();
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![item];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();

    let result = dispatch_new_work(
        &mut items,
        &config,
        10,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &HashMap::new(),
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    assert_eq!(result, 10);
    assert!(dry.is_empty());
}

#[tokio::test]
async fn dispatch_dry_run_blocks_at_in_progress_per_state_cap() {
    let pool = test_pool().await;

    // Two queued items, no per-state limits would let both spawn (max=10).
    // With a per-state cap of 1 on `in-progress`, only one should spawn.
    let mut first = Task::new("First");
    first.id = 1;
    first.status = ItemStatus::Queued;
    let mut second = Task::new("Second");
    second.id = 2;
    second.status = ItemStatus::Queued;

    let config = Config::default();
    let mut workflow = CaptainWorkflow::compiled_default();
    workflow
        .agent
        .per_state_limits
        .insert("in-progress".into(), 1);
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![first, second];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();

    let result = dispatch_new_work(
        &mut items,
        &config,
        0,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &HashMap::new(),
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    assert_eq!(result, 1, "only one item should spawn under cap=1");
    assert_eq!(dry.len(), 1);
    assert!(dry[0].contains("First"));
    assert!(alerts.is_empty());
}

#[tokio::test]
async fn dispatch_dry_run_per_state_cap_counts_existing_workers() {
    let pool = test_pool().await;

    // One item is already InProgress with a worker session — it occupies
    // the in-progress slot. A queued item should still NOT spawn under
    // a cap of 1 because the live worker fills the budget.
    let mut live = Task::new("Live");
    live.id = 1;
    live.status = ItemStatus::InProgress;
    live.worker = Some("w-1".into());
    live.session_ids.worker = Some("sess-1".into());

    let mut queued = Task::new("Queued");
    queued.id = 2;
    queued.status = ItemStatus::Queued;

    let config = Config::default();
    let mut workflow = CaptainWorkflow::compiled_default();
    workflow
        .agent
        .per_state_limits
        .insert("in-progress".into(), 1);
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![live, queued];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();

    let result = dispatch_new_work(
        &mut items,
        &config,
        1,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &HashMap::new(),
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    // Live worker stays counted; no new spawns.
    assert_eq!(result, 1);
    assert!(dry.is_empty(), "queued item must defer under per-state cap");
}

/// Combined behaviour the brief calls out: clarifier defers under a
/// `clarifying` cap while worker dispatch keeps spawning Queued items in
/// the same `dispatch_new_work` call. Confirms the two state-counter
/// reads are independent and one cap doesn't accidentally short-circuit
/// the other lane.
#[tokio::test]
async fn dispatch_dry_run_clarifier_capped_but_worker_proceeds() {
    let pool = test_pool().await;

    // One Clarifying item already holds the clarifying=1 budget.
    let mut live_clar = Task::new("Live clarifier");
    live_clar.id = 1;
    live_clar.status = ItemStatus::Clarifying;
    live_clar.session_ids.clarifier = Some("sess-c".into());

    // One New item: should be deferred (clarifying cap full).
    let mut new_item = Task::new("New deferred");
    new_item.id = 2;
    new_item.status = ItemStatus::New;

    // One Queued item: should spawn (worker lane has plenty of room).
    let mut queued = Task::new("Queued spawns");
    queued.id = 3;
    queued.status = ItemStatus::Queued;

    let config = Config::default();
    let mut workflow = CaptainWorkflow::compiled_default();
    workflow
        .agent
        .per_state_limits
        .insert("clarifying".into(), 1);
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![live_clar, new_item, queued];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();

    let result = dispatch_new_work(
        &mut items,
        &config,
        0,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &HashMap::new(),
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    // Worker lane spawned the Queued item; clarifier lane added nothing
    // because the clarifying cap was full.
    assert_eq!(result, 1, "one Queued item should spawn");
    assert_eq!(dry.len(), 1, "got {dry:?}");
    assert!(
        dry[0].contains("Queued spawns"),
        "worker lane should win, got {:?}",
        dry
    );
}

#[tokio::test]
async fn dispatch_dry_run_reserves_resource_between_items() {
    let pool = test_pool().await;

    let mut first = Task::new("First");
    first.id = 1;
    first.status = ItemStatus::Queued;
    first.resource = Some("browser".into());

    let mut second = Task::new("Second");
    second.id = 2;
    second.status = ItemStatus::Queued;
    second.resource = Some("browser".into());

    let config = Config::default();
    let workflow = CaptainWorkflow::compiled_default();
    let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
    let mut items = vec![first, second];
    let mut dry = Vec::new();
    let mut alerts = Vec::new();
    let mut resource_limits = HashMap::new();
    resource_limits.insert("browser".to_string(), 1usize);

    let result = dispatch_new_work(
        &mut items,
        &config,
        0,
        10,
        &workflow,
        &notifier,
        true,
        &mut dry,
        &mut alerts,
        &resource_limits,
        &pool,
        None,
        &tokio_util::task::TaskTracker::new(),
    )
    .await;

    assert_eq!(result, 1);
    assert_eq!(dry.len(), 1);
    assert!(dry[0].contains("First"));
    assert!(alerts.is_empty());
}
