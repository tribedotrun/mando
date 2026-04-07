use super::*;
use crate::AppState;

async fn test_state() -> AppState {
    let config = mando_config::Config::default();
    let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
    mando_config::set_active_captain_runtime_paths(runtime_paths.clone());
    let bus = mando_shared::EventBus::new();
    let db = mando_db::Db::open_in_memory().await.unwrap();
    let db = Arc::new(db);

    let cc_state_dir = std::env::temp_dir().join(format!(
        "mando-gw-test-cc-sessions-{:?}",
        std::thread::current().id()
    ));
    let cc_session_mgr = mando_captain::io::cc_session::CcSessionManager::new(
        cc_state_dir,
        "sonnet",
        db.pool().clone(),
    );
    let task_store = mando_captain::io::task_store::TaskStore::new(db.pool().clone());
    let config_write_mu = Arc::new(Mutex::new(()));
    let (tick_tx, _tick_rx) = watch::channel(crate::config_manager::initial_tick_duration(&config));
    let config_arc = Arc::new(ArcSwap::from_pointee(config));
    let config_manager = crate::config_manager::ConfigManager::new(
        config_arc.clone(),
        config_write_mu.clone(),
        tick_tx,
    );

    AppState {
        config: config_arc,
        config_manager,
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(
            mando_config::CaptainWorkflow::compiled_default(),
        )),
        scout_workflow: Arc::new(ArcSwap::from_pointee(
            mando_config::ScoutWorkflow::compiled_default(),
        )),
        config_write_mu,
        bus: Arc::new(bus),
        cc_session_mgr: Arc::new(cc_session_mgr),
        task_store: Arc::new(RwLock::new(task_store)),
        db,
        qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
        start_time: std::time::Instant::now(),
        listen_port: 0,
        dev_mode: false,
        task_tracker: TaskTracker::new(),
        cancellation_token: CancellationToken::new(),
        telegram_runtime: Arc::new(crate::telegram_runtime::TelegramRuntime::new(
            0,
            "test-token".into(),
        )),
        ui_runtime: Arc::new(crate::ui_runtime::UiRuntime::new(
            std::env::temp_dir().join("mando-ui-state-test.json"),
        )),
    }
}

#[tokio::test]
async fn health_endpoint() {
    let state = test_state().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("http://{addr}/api/health");
    // Give server a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["healthy"], true);
}

#[tokio::test]
async fn cors_headers_present() {
    let state = test_state().await;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let origin = "http://127.0.0.1:15173";
    let resp = client
        .get(format!("http://{addr}/api/health"))
        .header(reqwest::header::ORIGIN, origin)
        .send()
        .await
        .unwrap();

    let cors = resp
        .headers()
        .get("access-control-allow-origin")
        .map(|v| v.to_str().unwrap_or(""));
    assert_eq!(cors, Some(origin));

    let credentials = resp.headers().get("access-control-allow-credentials");
    assert_eq!(credentials, None);
}
