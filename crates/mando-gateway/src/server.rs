//! Axum app setup — router, CORS middleware, and server bind.

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::http::{header, Method};
use axum::routing::{delete, get, patch, post, put};
use axum::{middleware, Router};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::auth;
use crate::middleware::request_id;
use crate::routes_analytics;
use crate::routes_captain;
use crate::routes_captain_adopt;
use crate::routes_channels;
use crate::routes_clarifier;
use crate::routes_client_logs;
use crate::routes_config;
use crate::routes_cron;
use crate::routes_knowledge;
use crate::routes_ops;
use crate::routes_projects;
use crate::routes_scout;
use crate::routes_scout_bulk;
use crate::routes_scout_telegraph;
use crate::routes_sessions;
use crate::routes_task_actions;
use crate::routes_task_detail;
use crate::routes_tasks;
use crate::routes_voice;
use crate::routes_worktrees;
use crate::sse;
use crate::static_files;
use crate::AppState;

/// Build the full axum router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin.to_str().ok().is_some_and(|value| {
                value.starts_with("http://127.0.0.1:") || value.starts_with("http://localhost:")
            })
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    public_routes()
        .merge(
            Router::new()
                .route("/api/events", get(sse::sse_events))
                .route_layer(middleware::from_fn(auth::require_auth)),
        )
        .merge(protected_routes())
        .layer(middleware::from_fn(request_id::inject_request_id))
        .layer(cors)
        .with_state(state)
}

fn public_routes() -> Router<AppState> {
    Router::new().route("/api/health", get(routes_captain::get_health))
}

fn protected_routes() -> Router<AppState> {
    task_routes()
        .merge(captain_routes())
        .merge(scout_routes())
        .merge(cron_routes())
        .merge(session_routes())
        .merge(knowledge_routes())
        .merge(ops_routes())
        .merge(config_routes())
        .merge(channel_routes())
        .merge(voice_routes())
        .merge(worktree_routes())
        .merge(project_routes())
        .merge(analytics_routes())
        .route("/api/health/system", get(routes_captain::get_health_system))
        .route("/api/images/{filename}", get(static_files::get_image))
        .route(
            "/api/client-logs",
            post(routes_client_logs::post_client_logs),
        )
        .route_layer(middleware::from_fn(auth::require_auth))
}

fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", get(routes_tasks::get_tasks))
        .route("/api/tasks", delete(routes_tasks::delete_task_items))
        .route(
            "/api/tasks/{id}/history",
            get(routes_task_detail::get_task_history),
        )
        .route(
            "/api/tasks/{id}/timeline",
            get(routes_task_detail::get_task_timeline),
        )
        .route(
            "/api/tasks/{id}/pr-summary",
            get(routes_task_detail::get_task_pr_summary),
        )
        .route(
            "/api/tasks/{id}/sessions",
            get(routes_task_detail::get_task_sessions),
        )
        .route("/api/tasks/{id}", patch(routes_tasks::patch_task_item))
        .route("/api/tasks/add", post(routes_tasks::post_task_add))
        .route("/api/tasks/bulk", post(routes_tasks::post_task_bulk))
        .route("/api/tasks/delete", post(routes_tasks::post_task_delete))
        .route("/api/tasks/merge", post(routes_tasks::post_task_merge))
        .route(
            "/api/tasks/accept",
            post(routes_task_actions::post_task_accept),
        )
        .route(
            "/api/tasks/cancel",
            post(routes_task_actions::post_task_cancel),
        )
        .route(
            "/api/tasks/reopen",
            post(routes_task_actions::post_task_reopen),
        )
        .route(
            "/api/tasks/rework",
            post(routes_task_actions::post_task_rework),
        )
        .route(
            "/api/tasks/handoff",
            post(routes_task_actions::post_task_handoff),
        )
        .route("/api/tasks/ask", post(routes_task_actions::post_task_ask))
        .route(
            "/api/tasks/retry",
            post(routes_task_actions::post_task_retry),
        )
        .route(
            "/api/tasks/{id}/clarify",
            post(routes_clarifier::post_task_clarify),
        )
        .route(
            "/api/tasks/{id}/archive",
            post(routes_task_actions::post_task_archive),
        )
        .route(
            "/api/tasks/{id}/unarchive",
            post(routes_task_actions::post_task_unarchive),
        )
}

fn captain_routes() -> Router<AppState> {
    Router::new()
        .route("/api/captain/tick", post(routes_captain::post_captain_tick))
        .route(
            "/api/captain/triage",
            post(routes_captain::post_captain_triage),
        )
        .route(
            "/api/captain/merge",
            post(routes_captain::post_captain_merge),
        )
        .route("/api/captain/stop", post(routes_captain::post_captain_stop))
        .route(
            "/api/captain/nudge",
            post(routes_captain::post_captain_nudge),
        )
        .route(
            "/api/captain/adopt",
            post(routes_captain_adopt::post_captain_adopt),
        )
        .route("/api/workers", get(routes_captain::get_workers))
        .route("/api/workers/{id}", get(routes_captain::get_worker))
        .route(
            "/api/workers/{id}/kill",
            post(routes_captain::post_worker_kill),
        )
}

fn scout_routes() -> Router<AppState> {
    Router::new()
        .route("/api/scout/items", get(routes_scout::get_scout_items))
        .route("/api/scout/items", post(routes_scout::post_scout_items))
        .route("/api/scout/items/{id}", get(routes_scout::get_scout_item))
        .route(
            "/api/scout/items/{id}",
            patch(routes_scout::patch_scout_item),
        )
        .route(
            "/api/scout/items/{id}",
            delete(routes_scout::delete_scout_item),
        )
        .route(
            "/api/scout/items/{id}/article",
            get(routes_scout::get_scout_article),
        )
        .route(
            "/api/scout/items/{id}/telegraph",
            post(routes_scout_telegraph::publish_telegraph),
        )
        .route(
            "/api/scout/items/{id}/act",
            post(routes_scout::post_scout_act),
        )
        .route(
            "/api/scout/items/{id}/sessions",
            get(routes_scout::get_scout_item_sessions),
        )
        .route("/api/scout/process", post(routes_scout::post_scout_process))
        .route(
            "/api/scout/research",
            post(routes_scout::post_scout_research),
        )
        .route("/api/scout/ask", post(routes_scout::post_scout_ask))
        .route(
            "/api/scout/bulk",
            post(routes_scout_bulk::post_scout_bulk_update),
        )
        .route(
            "/api/scout/bulk-delete",
            post(routes_scout_bulk::post_scout_bulk_delete),
        )
}

fn cron_routes() -> Router<AppState> {
    Router::new()
        .route("/api/cron", get(routes_cron::get_cron))
        .route("/api/cron/add", post(routes_cron::post_cron_add))
        .route("/api/cron/remove", post(routes_cron::post_cron_remove))
        .route("/api/cron/toggle", post(routes_cron::post_cron_toggle))
        .route("/api/cron/run", post(routes_cron::post_cron_run))
}

fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions", get(routes_sessions::get_sessions))
        .route(
            "/api/sessions/{id}/transcript",
            get(routes_sessions::get_session_transcript),
        )
        .route(
            "/api/sessions/{id}/messages",
            get(routes_sessions::get_session_messages),
        )
        .route(
            "/api/sessions/{id}/tools",
            get(routes_sessions::get_session_tools),
        )
        .route(
            "/api/sessions/{id}/cost",
            get(routes_sessions::get_session_cost),
        )
}

fn knowledge_routes() -> Router<AppState> {
    Router::new()
        .route("/api/knowledge", get(routes_knowledge::get_knowledge))
        .route(
            "/api/knowledge/pending",
            get(routes_knowledge::get_knowledge_pending),
        )
        .route(
            "/api/knowledge/approve",
            post(routes_knowledge::post_knowledge_approve),
        )
        .route(
            "/api/knowledge/learn",
            post(routes_knowledge::post_knowledge_learn),
        )
        .route(
            "/api/self-improve/trigger",
            post(routes_knowledge::post_self_improve_trigger),
        )
        // Journal & patterns
        .route("/api/journal", get(routes_knowledge::get_journal))
        .route("/api/patterns", get(routes_knowledge::get_patterns))
        .route(
            "/api/patterns/update",
            post(routes_knowledge::post_pattern_update),
        )
}

fn ops_routes() -> Router<AppState> {
    Router::new()
        .route("/api/ops/start", post(routes_ops::post_ops_start))
        .route("/api/ops/message", post(routes_ops::post_ops_message))
        .route("/api/ops/end", post(routes_ops::post_ops_end))
}

fn config_routes() -> Router<AppState> {
    Router::new()
        .route("/api/config", get(routes_config::get_config))
        .route("/api/config", put(routes_config::put_config))
        .route("/api/config/status", get(routes_config::get_config_status))
        .route("/api/config/setup", post(routes_config::post_config_setup))
        .route("/api/config/paths", get(routes_config::get_config_paths))
}

fn channel_routes() -> Router<AppState> {
    Router::new()
        .route("/api/channels", get(routes_channels::get_channels))
        .route("/api/notify", post(routes_channels::post_notify))
        .route(
            "/api/firecrawl/scrape",
            post(routes_channels::post_firecrawl_scrape),
        )
}

fn voice_routes() -> Router<AppState> {
    Router::new()
        .route("/api/voice", post(routes_voice::post_voice))
        .route("/api/voice/usage", get(routes_voice::get_voice_usage))
        .route("/api/voice/sessions", get(routes_voice::get_voice_sessions))
        .route(
            "/api/voice/sessions/{id}/messages",
            get(routes_voice::get_voice_messages),
        )
        .route(
            "/api/voice/transcribe",
            post(routes_voice::post_voice_transcribe),
        )
}

fn worktree_routes() -> Router<AppState> {
    Router::new()
        .route("/api/worktrees", get(routes_worktrees::get_worktrees))
        .route("/api/worktrees", post(routes_worktrees::post_worktrees))
        .route(
            "/api/worktrees/prune",
            post(routes_worktrees::post_worktrees_prune),
        )
        .route(
            "/api/worktrees/remove",
            post(routes_worktrees::post_worktrees_remove),
        )
        .route(
            "/api/worktrees/cleanup",
            post(routes_worktrees::post_worktrees_cleanup),
        )
}

fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects", get(routes_projects::get_projects))
        .route("/api/projects", post(routes_projects::post_projects))
        .route(
            "/api/projects/{name}",
            patch(routes_projects::patch_project),
        )
        .route(
            "/api/projects/{name}",
            delete(routes_projects::delete_project),
        )
}

fn analytics_routes() -> Router<AppState> {
    Router::new().route("/api/analytics", get(routes_analytics::get_analytics))
}

/// Start the gateway HTTP server.
pub async fn start_server(
    config: mando_config::Config,
    bus: mando_shared::EventBus,
) -> anyhow::Result<()> {
    let host = config.gateway.dashboard.host.clone();
    let port = config.gateway.dashboard.port;
    let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
    mando_config::set_active_captain_runtime_paths(runtime_paths.clone());

    let bus_arc = Arc::new(bus);

    // Unified DB pool — shared across all subsystems.
    let db = mando_db::Db::open(&runtime_paths.task_db_path).await?;
    let db = Arc::new(db);
    let task_store = mando_captain::io::task_store::TaskStore::new(db.pool().clone());
    let task_store_arc = Arc::new(RwLock::new(task_store));
    let config_arc = Arc::new(ArcSwap::from_pointee(config));

    // Cron service: wire callback before start.
    let mut cron_service = mando_shared::CronService::new(db.pool().clone());
    cron_service.set_on_job(crate::cron_executor::make_cron_callback(
        config_arc.clone(),
        task_store_arc.clone(),
        bus_arc.clone(),
    ));
    cron_service.start().await;

    let config = config_arc.load_full();
    let tick_interval_s = config.captain.tick_interval_s.max(10);
    let learn_cron_expr = config.captain.learn_cron_expr.clone();

    // Clean dead PIDs first so reconciliation sees accurate liveness state.
    mando_captain::io::pid_registry::cleanup_dead();

    // Reconcile incomplete operations from previous run (WAL recovery).
    if let Err(e) =
        mando_captain::runtime::reconciler::reconcile_on_startup(&config, db.pool()).await
    {
        tracing::error!(module = "startup", error = %e, "reconciliation failed");
    }

    let captain_wf = mando_config::load_captain_workflow(
        &mando_config::captain_workflow_path(),
        config.captain.tick_interval_s,
    );
    let scout_wf = mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &config);
    let voice_wf = mando_config::load_voice_workflow(&mando_config::voice_workflow_path());

    let cc_state_dir = mando_config::state_dir().join("ops_sessions").join("cc");
    let mut cc_session_mgr = mando_captain::io::cc_session::CcSessionManager::new(
        cc_state_dir,
        "sonnet",
        db.pool().clone(),
    );
    cc_session_mgr.recover();

    let state = AppState {
        config: config_arc,
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(captain_wf)),
        scout_workflow: Arc::new(ArcSwap::from_pointee(scout_wf)),
        voice_workflow: Arc::new(ArcSwap::from_pointee(voice_wf)),
        config_write_mu: Arc::new(Mutex::new(())),
        bus: bus_arc.clone(),
        cron_service: Arc::new(RwLock::new(cron_service)),
        cc_session_mgr: Arc::new(RwLock::new(cc_session_mgr)),
        task_store: task_store_arc,
        db,
        linear_workspace_slug: Arc::new(RwLock::new(None)),
        qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
        start_time: std::time::Instant::now(),
    };

    // Fetch Linear workspace slug in background.
    crate::spawn_linear_slug_fetch(state.config.clone(), state.linear_workspace_slug.clone());

    // Spawn captain tick loop (always runs; respects auto_schedule dynamically).
    crate::background_tasks::spawn_auto_tick(&state, tick_interval_s);

    // Spawn distiller cron loop (always runs; respects auto_schedule dynamically).
    crate::background_tasks::spawn_distiller_cron(
        state.config.clone(),
        state.captain_workflow.clone(),
        state.bus.clone(),
        state.db.pool().clone(),
        &learn_cron_expr,
    );

    let app = build_router(state);
    let addr = format!("{host}:{port}");
    info!("gateway listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;

    async fn test_state() -> AppState {
        let config = mando_config::Config::default();
        let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
        mando_config::set_active_captain_runtime_paths(runtime_paths.clone());
        let bus = mando_shared::EventBus::new();
        let db = mando_db::Db::open_in_memory().await.unwrap();
        let db = Arc::new(db);
        let cron_service = mando_shared::CronService::new(db.pool().clone());

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

        AppState {
            config: Arc::new(ArcSwap::from_pointee(config)),
            runtime_paths,
            captain_workflow: Arc::new(ArcSwap::from_pointee(
                mando_config::CaptainWorkflow::compiled_default(),
            )),
            scout_workflow: Arc::new(ArcSwap::from_pointee(
                mando_config::ScoutWorkflow::compiled_default(),
            )),
            voice_workflow: Arc::new(ArcSwap::from_pointee(
                mando_config::VoiceWorkflow::compiled_default(),
            )),
            config_write_mu: Arc::new(Mutex::new(())),
            bus: Arc::new(bus),
            cron_service: Arc::new(RwLock::new(cron_service)),
            cc_session_mgr: Arc::new(RwLock::new(cc_session_mgr)),
            task_store: Arc::new(RwLock::new(task_store)),
            db,
            linear_workspace_slug: Arc::new(RwLock::new(None)),
            qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
            start_time: std::time::Instant::now(),
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
        let origin = "http://127.0.0.1:5173";
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
}
