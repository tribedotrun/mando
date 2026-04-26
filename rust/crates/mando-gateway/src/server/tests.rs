use super::*;
use crate::AppState;
use base64::Engine;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

fn test_data_dir() -> PathBuf {
    let data_dir = std::env::temp_dir().join(format!(
        "mando-gw-terminal-test-{}",
        global_infra::uuid::Uuid::v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    data_dir
}

fn isolate_auth_data_dir() -> (PathBuf, global_infra::EnvVarGuard) {
    let data_dir = test_data_dir();
    let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &data_dir);
    (data_dir, guard)
}

async fn test_state() -> AppState {
    test_state_with_data_dir(test_data_dir()).await
}

struct TestStateParts {
    state: AppState,
    db: Arc<global_db::Db>,
    task_store: Arc<RwLock<captain::TaskStore>>,
}

async fn test_state_with_data_dir(data_dir: PathBuf) -> AppState {
    test_state_parts_with_data_dir(data_dir).await.state
}

async fn test_state_parts_with_data_dir(data_dir: PathBuf) -> TestStateParts {
    let config = settings::Config::default();
    let runtime_paths = captain::resolve_captain_runtime_paths(&config);
    captain::set_active_captain_runtime_paths(runtime_paths.clone());
    let bus = Arc::new(global_bus::EventBus::new());
    let db = global_db::Db::open_in_memory().await.unwrap();
    let db = Arc::new(db);
    let test_id = global_infra::uuid::Uuid::v4();

    let cc_state_dir = std::env::temp_dir().join(format!("mando-gw-test-cc-sessions-{test_id}"));
    std::fs::create_dir_all(&cc_state_dir).unwrap();
    let sessions_runtime =
        crate::session_backend::build_sessions_runtime(cc_state_dir, "sonnet", db.pool().clone());
    let task_store = Arc::new(RwLock::new(captain::TaskStore::new(db.pool().clone())));
    let settings = Arc::new(settings::SettingsRuntime::new_with_loaded(
        config,
        settings::CaptainWorkflow::compiled_default(),
        settings::ScoutWorkflow::compiled_default(),
        db.pool().clone(),
        settings::WorkflowRuntimeMode::Normal,
    ));
    let task_tracker = TaskTracker::new();
    let cancellation_token = CancellationToken::new();
    let ui_runtime = Arc::new(transport_ui::UiRuntime::new(
        std::env::temp_dir().join(format!("mando-ui-state-test-{test_id}.json")),
    ));
    let telegram_runtime = Arc::new(transport_tg::TelegramRuntime::new(0, "test-token".into()));
    let qa_session_mgr =
        scout::session_manager_from_workflow(&settings::ScoutWorkflow::compiled_default());
    let terminal_host = Arc::new(terminal::TerminalHost::new(data_dir));
    let captain_runtime = Arc::new(captain::CaptainRuntime::new(captain::CaptainRuntimeDeps {
        settings: settings.clone(),
        bus: bus.clone(),
        task_store: task_store.clone(),
        pool: db.pool().clone(),
        task_tracker: task_tracker.clone(),
        cancellation_token: cancellation_token.clone(),
        auto_title_notify: Arc::new(tokio::sync::Notify::new()),
        cleanup_expired_sessions: {
            let sessions_runtime = sessions_runtime.clone();
            Arc::new(move || sessions_runtime.cleanup_expired())
        },
    }));
    let scout_runtime = Arc::new(scout::ScoutRuntime::new(
        settings.clone(),
        bus.clone(),
        db.pool().clone(),
        Arc::new(tokio::sync::Semaphore::new(4)),
        task_tracker.clone(),
        cancellation_token.clone(),
        qa_session_mgr.clone(),
    ));
    let terminal_runtime = Arc::new(terminal::TerminalRuntime::new(
        terminal_host,
        settings.clone(),
        0,
        task_tracker.clone(),
        cancellation_token.clone(),
        Arc::new(transport_http::ensure_auth_token),
    ));

    let state = AppState {
        settings,
        runtime_paths,
        bus,
        captain: captain_runtime,
        scout: scout_runtime,
        sessions: sessions_runtime,
        terminal: terminal_runtime,
        start_time: std::time::Instant::now(),
        listen_port: 0,
        task_tracker,
        cancellation_token,
        telegram_runtime,
        ui_runtime,
    };

    TestStateParts {
        state,
        db,
        task_store,
    }
}

async fn spawn_app(state: AppState) -> std::net::SocketAddr {
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let health_url = format!("http://{addr}/api/health");
    for _ in 0..50 {
        if let Ok(resp) = reqwest::get(&health_url).await {
            if resp.status().is_success() {
                return addr;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("test server did not become healthy: {health_url}");
}

fn authed_client() -> reqwest::Client {
    let token = transport_http::ensure_auth_token();
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
    let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
    let (data_dir, _auth_guard) = isolate_auth_data_dir();
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
    // Restored sessions have no scrollback in output_buf, so only the typed
    // exit envelope is emitted.
    assert!(!body.contains(r#""event":"output""#));
    assert!(body.contains(r#""event":"exit""#));
}

#[tokio::test]
async fn terminal_stream_replay_zero_skips_snapshot_for_restored_history() {
    let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
    let (data_dir, _auth_guard) = isolate_auth_data_dir();
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
    assert!(!body.contains(r#""event":"output""#));
    assert!(!body.contains(&base64::engine::general_purpose::STANDARD.encode("skip me")));
    assert!(body.contains(r#""event":"exit""#));
}

#[tokio::test]
async fn sessions_endpoint_returns_runtime_backed_enrichment() {
    let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
    let (data_dir, _auth_guard) = isolate_auth_data_dir();
    let TestStateParts {
        state,
        db,
        task_store,
    } = test_state_parts_with_data_dir(data_dir).await;
    let project_id = settings::projects::upsert(
        db.pool(),
        "test",
        "/tmp/test-project",
        Some("acme/test-project"),
    )
    .await
    .unwrap();

    let mut task = captain::Task::new("Runtime-backed task title");
    task.project_id = project_id;
    task.project = "test".into();
    // Seed a workbench row so the new tasks.workbench_id NOT NULL FK is satisfied.
    let now = global_types::now_rfc3339();
    let wb_id: i64 = sqlx::query_scalar(
        "INSERT INTO workbenches (project_id, worktree, title, created_at, last_activity_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(project_id)
    .bind(format!(
        "/tmp/mando-test-wb-{}",
        global_infra::uuid::Uuid::v4()
    ))
    .bind("test-workbench")
    .bind(&now)
    .bind(&now)
    .fetch_one(db.pool())
    .await
    .unwrap();
    task.workbench_id = wb_id;
    let task_id = {
        let store = task_store.read().await;
        store.add(task).await.unwrap()
    };

    sessions::queries::upsert_session(
        db.pool(),
        &sessions::queries::SessionUpsert {
            session_id: "sess-runtime-1",
            created_at: "2026-04-17T00:00:00Z",
            caller: "ops",
            cwd: "/tmp/test-project",
            model: "sonnet",
            status: global_types::SessionStatus::Running,
            cost_usd: Some(1.25),
            duration_ms: Some(42),
            resumed: false,
            task_id: Some(task_id),
            scout_item_id: None,
            worker_name: None,
            resumed_at: None,
            credential_id: None,
            error: None,
            api_error_status: None,
        },
    )
    .await
    .unwrap();

    let addr = spawn_app(state).await;
    let client = authed_client();

    let body: serde_json::Value = client
        .get(format!("http://{addr}/api/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["total"], 1);
    assert_eq!(body["sessions"][0]["session_id"], "sess-runtime-1");
    assert_eq!(
        body["sessions"][0]["task_title"],
        "Runtime-backed task title"
    );
    assert_eq!(body["sessions"][0]["cost_usd"], 1.25);
}
