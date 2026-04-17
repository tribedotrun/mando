use super::*;
use crate::AppState;
use base64::Engine;
use std::path::PathBuf;

fn test_data_dir() -> PathBuf {
    let data_dir = std::env::temp_dir().join(format!(
        "mando-gw-terminal-test-{}",
        global_infra::uuid::Uuid::v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    data_dir
}

async fn test_state() -> AppState {
    test_state_with_data_dir(test_data_dir()).await
}

async fn test_state_with_data_dir(data_dir: PathBuf) -> AppState {
    let config = settings::config::Config::default();
    let runtime_paths = captain::config::resolve_captain_runtime_paths(&config);
    captain::config::set_active_captain_runtime_paths(runtime_paths.clone());
    let bus = global_bus::EventBus::new();
    let db = global_db::Db::open_in_memory().await.unwrap();
    let db = Arc::new(db);

    let cc_state_dir = std::env::temp_dir().join(format!(
        "mando-gw-test-cc-sessions-{:?}",
        std::thread::current().id()
    ));
    let cc_session_mgr =
        captain::io::cc_session::CcSessionManager::new(cc_state_dir, "sonnet", db.pool().clone());
    let task_store = captain::io::task_store::TaskStore::new(db.pool().clone());
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
            settings::config::CaptainWorkflow::compiled_default(),
        )),
        scout_workflow: Arc::new(ArcSwap::from_pointee(
            settings::config::ScoutWorkflow::compiled_default(),
        )),
        config_write_mu,
        bus: Arc::new(bus),
        cc_session_mgr: Arc::new(cc_session_mgr),
        task_store: Arc::new(RwLock::new(task_store)),
        credential_mgr: Arc::new(crate::credentials::CredentialManager::new(
            db.pool().clone(),
        )),
        db,
        qa_session_mgr: scout::runtime::qa::session_manager_from_workflow(
            &settings::config::ScoutWorkflow::compiled_default(),
        ),
        terminal_host: Arc::new(terminal::TerminalHost::new(data_dir)),
        start_time: std::time::Instant::now(),
        listen_port: 0,
        dev_mode: false,
        sandbox_mode: false,
        task_tracker: TaskTracker::new(),
        cancellation_token: CancellationToken::new(),
        telegram_runtime: Arc::new(crate::telegram_runtime::TelegramRuntime::new(
            0,
            "test-token".into(),
        )),
        ui_runtime: Arc::new(crate::ui_runtime::UiRuntime::new(
            std::env::temp_dir().join("mando-ui-state-test.json"),
        )),
        scout_processing_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
        auto_title_notify: Arc::new(tokio::sync::Notify::new()),
    }
}

async fn spawn_app(state: AppState) -> std::net::SocketAddr {
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    addr
}

fn authed_client() -> reqwest::Client {
    let token = transport_http::auth::ensure_auth_token();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

fn seed_terminal_history(data_dir: &std::path::Path, id: &str, _output: &[u8], clean_exit: bool) {
    let session_dir = data_dir.join("terminal-history").join(id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = serde_json::json!({
        "id": id,
        "project": "mando",
        "cwd": data_dir,
        "agent": "claude",
        "terminal_id": "wb:test",
        "created_at": "2026-04-08T00:00:00Z",
        "ended_at": clean_exit.then_some("2026-04-08T00:05:00Z"),
        "exit_code": clean_exit.then_some(0),
        "size": { "rows": 24, "cols": 80 },
        "state": if clean_exit { "exited" } else { "live" }
    });
    std::fs::write(
        session_dir.join("meta.json"),
        serde_json::to_vec_pretty(&meta).unwrap(),
    )
    .unwrap();
    // scrollback.bin no longer persisted; output param is ignored for restored sessions
}

#[tokio::test]
async fn health_endpoint() {
    let addr = spawn_app(test_state().await).await;
    let url = format!("http://{addr}/api/health");

    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["healthy"], true);
}

#[tokio::test]
async fn cors_headers_present() {
    let addr = spawn_app(test_state().await).await;

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

#[tokio::test]
async fn terminal_list_and_stream_restore_unclean_history() {
    let data_dir = test_data_dir();
    seed_terminal_history(&data_dir, "restore-1", b"hello restored", false);
    let addr = spawn_app(test_state_with_data_dir(data_dir).await).await;
    let client = authed_client();

    let sessions: serde_json::Value = client
        .get(format!("http://{addr}/api/terminal"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 1);
    assert_eq!(sessions[0]["state"], "restored");
    assert_eq!(sessions[0]["restored"], true);

    let body = client
        .get(format!("http://{addr}/api/terminal/restore-1/stream"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    // Restored sessions have no scrollback in output_buf, so only the exit event is emitted
    assert!(!body.contains("event: output"));
    assert!(body.contains("event: exit"));
}

#[tokio::test]
async fn terminal_stream_replay_zero_skips_snapshot_for_restored_history() {
    let data_dir = test_data_dir();
    seed_terminal_history(&data_dir, "restore-2", b"skip me", false);
    let addr = spawn_app(test_state_with_data_dir(data_dir).await).await;
    let client = authed_client();

    let body = client
        .get(format!(
            "http://{addr}/api/terminal/restore-2/stream?replay=0"
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!body.contains("event: output"));
    assert!(!body.contains(&base64::engine::general_purpose::STANDARD.encode("skip me")));
    assert!(body.contains("event: exit"));
}
