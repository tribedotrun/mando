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
