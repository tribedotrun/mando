use arc_swap::ArcSwap;
use axum::http::{header, Method};
use axum::routing::{delete, get, patch, post, put};
use axum::{middleware, Router};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::auth;
use crate::middleware::request_id;
use crate::routes_ai;
use crate::routes_captain;
use crate::routes_captain_adopt;
use crate::routes_channels;
use crate::routes_clarifier;
use crate::routes_client_logs;
use crate::routes_config;
use crate::routes_ops;
use crate::routes_projects;
use crate::routes_scout;
use crate::routes_scout_bulk;
use crate::routes_scout_telegraph;
use crate::routes_sessions;
use crate::routes_task_actions;
use crate::routes_task_ask;
use crate::routes_task_detail;
use crate::routes_tasks;
use crate::routes_terminal;
use crate::routes_ui;
use crate::routes_workbenches;
use crate::routes_worktrees;
use crate::sse;
use crate::static_files;
use crate::telegram_runtime;
use crate::ui_runtime;
use crate::AppState;

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
        .merge(session_routes())
        .merge(ops_routes())
        .merge(config_routes())
        .merge(channel_routes())
        .merge(worktree_routes())
        .merge(routes_workbenches::routes())
        .merge(project_routes())
        .merge(ai_routes())
        .merge(routes_terminal::routes())
        .merge(ui_routes())
        .route("/api/health/system", get(routes_captain::get_health_system))
        .route("/api/health/ui", get(routes_ui::get_ui_health))
        .route("/api/health/telegram", get(routes_ui::get_telegram_health))
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
        .route("/api/tasks/ask", post(routes_task_ask::post_task_ask))
        .route(
            "/api/tasks/ask/end",
            post(routes_task_ask::post_task_ask_end),
        )
        .route(
            "/api/tasks/retry",
            post(routes_task_actions::post_task_retry),
        )
        .route(
            "/api/tasks/{id}/clarify",
            post(routes_clarifier::post_task_clarify),
        )
}
fn captain_routes() -> Router<AppState> {
    Router::new()
        .route("/api/captain/tick", post(routes_captain::post_captain_tick))
        .route(
            "/api/captain/triage",
            post(routes_captain::post_captain_triage),
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
        .route(
            "/api/channels/telegram/owner",
            post(routes_channels::post_telegram_owner),
        )
        .route(
            "/api/telegram/restart",
            post(routes_channels::post_telegram_restart),
        )
        .route("/api/notify", post(routes_channels::post_notify))
        .route(
            "/api/firecrawl/scrape",
            post(routes_channels::post_firecrawl_scrape),
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

fn ai_routes() -> Router<AppState> {
    Router::new().route("/api/ai/parse-todos", post(routes_ai::post_parse_todos))
}

fn ui_routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/register", post(routes_ui::post_ui_register))
        .route("/api/ui/quitting", post(routes_ui::post_ui_quitting))
        .route("/api/ui/updating", post(routes_ui::post_ui_updating))
        .route("/api/ui/launch", post(routes_ui::post_ui_launch))
        .route("/api/ui/restart", post(routes_ui::post_ui_restart))
}

pub async fn start_server(
    cfg: mando_config::Config,
    bus: mando_shared::EventBus,
) -> anyhow::Result<()> {
    start_server_with(cfg, bus, std::future::pending::<()>(), false).await
}

/// Full `start_server` with explicit shutdown signal and unsafe-start gate.
pub async fn start_server_with<F>(
    config: mando_config::Config,
    bus: mando_shared::EventBus,
    shutdown: F,
    unsafe_start: bool,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let host = config.gateway.dashboard.host.clone();
    let port = config.gateway.dashboard.port;
    let runtime_paths = mando_config::resolve_captain_runtime_paths(&config);
    mando_config::set_active_captain_runtime_paths(runtime_paths.clone());

    let bus_arc = Arc::new(bus);

    let db = mando_db::Db::open(&runtime_paths.task_db_path).await?;
    let db = Arc::new(db);

    let config = {
        let mut cfg = config;
        mando_db::queries::projects::startup_sync(db.pool(), &mut cfg).await?;
        cfg
    };

    let task_store = mando_captain::io::task_store::TaskStore::new(db.pool().clone());
    let task_store_arc = Arc::new(RwLock::new(task_store));
    let config_arc = Arc::new(ArcSwap::from_pointee(config));
    let config_write_mu = Arc::new(Mutex::new(()));
    let (tick_tx, tick_rx) = watch::channel(crate::config_manager::initial_tick_duration(
        &config_arc.load_full(),
    ));
    let config_manager = crate::config_manager::ConfigManager::new(
        config_arc.clone(),
        config_write_mu.clone(),
        tick_tx,
    );
    let auth_token = crate::auth::ensure_auth_token();
    let task_tracker = TaskTracker::new();
    let cancellation_token = CancellationToken::new();
    let ui_runtime = Arc::new(ui_runtime::UiRuntime::new(
        mando_config::state_dir().join("ui-state.json"),
    ));
    let telegram_runtime = Arc::new(telegram_runtime::TelegramRuntime::new(port, auth_token));

    let config = config_arc.load_full();

    if let Err(e) = mando_captain::io::pid_registry::cleanup_dead() {
        tracing::warn!(module = "startup", error = %e, "pid_registry cleanup_dead failed");
    }

    if let Err(e) =
        mando_captain::runtime::reconciler::reconcile_on_startup(&config, db.pool()).await
    {
        if unsafe_start {
            tracing::error!(
                module = "startup",
                error = %e,
                "reconciliation failed — continuing under unsafe_start"
            );
        } else {
            tracing::error!(
                module = "startup",
                error = %e,
                "reconciliation failed — refusing to start (set MANDO_UNSAFE_START=1 to override)"
            );
            return Err(e);
        }
    }

    let captain_wf = mando_config::load_captain_workflow(
        &mando_config::captain_workflow_path(),
        config.captain.tick_interval_s,
    )?;
    let scout_wf =
        mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &config)?;
    let cc_state_dir = mando_config::state_dir().join("ops_sessions").join("cc");
    let cc_session_mgr = mando_captain::io::cc_session::CcSessionManager::new(
        cc_state_dir,
        "sonnet",
        db.pool().clone(),
    );
    let cc_recovered = cc_session_mgr.recover();
    if cc_recovered.recovered > 0 || cc_recovered.corrupt > 0 {
        tracing::info!(
            recovered = cc_recovered.recovered,
            corrupt = cc_recovered.corrupt,
            "recovered sessions from disk"
        );
    }

    let state = AppState {
        config: config_arc,
        config_manager,
        runtime_paths,
        captain_workflow: Arc::new(ArcSwap::from_pointee(captain_wf)),
        scout_workflow: Arc::new(ArcSwap::from_pointee(scout_wf)),
        config_write_mu,
        bus: bus_arc.clone(),
        cc_session_mgr: Arc::new(cc_session_mgr),
        task_store: task_store_arc,
        db,
        qa_session_mgr: mando_scout::runtime::qa::default_session_manager(),
        terminal_host: Arc::new(mando_terminal::TerminalHost::new()),
        start_time: std::time::Instant::now(),
        listen_port: port,
        dev_mode: false,
        task_tracker,
        cancellation_token,
        telegram_runtime,
        ui_runtime,
    };

    state
        .ui_runtime
        .start_monitor(&state.task_tracker, state.cancellation_token.clone());

    crate::background_tasks::spawn_auto_tick(&state, tick_rx);

    if let Err(err) = state.telegram_runtime.configure(&config).await {
        tracing::warn!(
            module = "telegram",
            error = %err,
            "failed to start embedded telegram runtime"
        );
    }

    let terminal_host = state.terminal_host.clone();
    let tracker = state.task_tracker.clone();
    let cancel = state.cancellation_token.clone();
    let app = build_router(state);
    let addr = format!("{host}:{port}");
    info!("gateway listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let graceful = async move {
        shutdown.await;
        cancel.cancel();
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await?;

    terminal_host.shutdown();
    tracker.close();
    tracker.wait().await;
    Ok(())
}

#[cfg(test)]
mod tests;
