//! mando-gateway — axum HTTP server for the Mando dashboard.
//!
//! Transport layer: thin handlers that parse requests, call domain
//! functions, and format JSON responses.

pub mod auth;
pub mod background_tasks;
pub mod cron_executor;
pub mod instance;
pub mod middleware;
pub(crate) mod response;
mod routes_analytics;
mod routes_captain;
mod routes_captain_adopt;
mod routes_channels;
mod routes_clarifier;
mod routes_client_logs;
mod routes_config;
mod routes_cron;
mod routes_knowledge;
mod routes_ops;
mod routes_projects;
mod routes_scout;
mod routes_scout_bulk;
mod routes_scout_telegraph;
mod routes_sessions;
mod routes_task_actions;
mod routes_task_detail;
mod routes_tasks;
mod routes_voice;
mod routes_worktrees;
pub mod server;
mod sse;
mod static_files;
pub mod telemetry;
pub(crate) mod voice;

use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio::sync::{Mutex, RwLock};

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<mando_config::Config>>,
    pub runtime_paths: mando_config::CaptainRuntimePaths,
    pub captain_workflow: Arc<ArcSwap<mando_config::CaptainWorkflow>>,
    pub scout_workflow: Arc<ArcSwap<mando_config::ScoutWorkflow>>,
    pub voice_workflow: Arc<ArcSwap<mando_config::VoiceWorkflow>>,
    /// Serializes config/workflow write operations (read-modify-write).
    /// ArcSwap provides lock-free reads but doesn't serialize writers —
    /// concurrent config saves need this mutex to prevent lost updates.
    pub config_write_mu: Arc<Mutex<()>>,
    pub bus: Arc<mando_shared::EventBus>,
    pub cron_service: Arc<RwLock<mando_shared::CronService>>,
    pub cc_session_mgr: Arc<RwLock<mando_captain::io::cc_session::CcSessionManager>>,
    pub task_store: Arc<RwLock<mando_captain::io::task_store::TaskStore>>,
    pub db: Arc<mando_db::Db>,
    pub linear_workspace_slug: Arc<RwLock<Option<String>>>,
    pub qa_session_mgr: Arc<mando_scout::runtime::qa::QaSessionManager>,
    pub start_time: Instant,
}

/// Resolve a project display-name to its `github_repo` slug from config.
pub(crate) use mando_config::resolve_github_repo;

pub(crate) fn captain_notifier(
    state: &AppState,
    config: &mando_config::Config,
) -> mando_captain::runtime::notify::Notifier {
    let default_slug = if config.captain.projects.len() == 1 {
        config
            .captain
            .projects
            .values()
            .next()
            .and_then(|pc| pc.github_repo.clone())
    } else {
        None
    };

    mando_captain::runtime::notify::Notifier::new(state.bus.clone())
        .with_repo_slug(default_slug)
        .with_notifications_enabled(true)
}

/// Fetch the Linear workspace `urlKey` from the GraphQL API.
pub async fn fetch_linear_slug(api_key: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let body = serde_json::json!({ "query": "{ organization { urlKey } }" });
    let resp = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Linear API returned {status} — check LINEAR_API_KEY: {body}"
        ));
    }
    let json: serde_json::Value = resp.json().await?;
    json["data"]["organization"]["urlKey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("urlKey not found in Linear response: {json}"))
}

/// Spawn a background task to fetch and cache the Linear workspace slug.
pub fn spawn_linear_slug_fetch(
    config: Arc<ArcSwap<mando_config::Config>>,
    slug: Arc<RwLock<Option<String>>>,
) {
    tokio::spawn(async move {
        let api_key = config.load().env.get("LINEAR_API_KEY").cloned();
        match api_key {
            Some(key) => match fetch_linear_slug(&key).await {
                Ok(s) => {
                    tracing::info!(module = "linear", slug = %s, "fetched workspace slug");
                    *slug.write().await = Some(s);
                }
                Err(e) => {
                    tracing::warn!(module = "linear", error = %e, "failed to fetch Linear workspace slug");
                }
            },
            None => {
                tracing::info!(
                    module = "linear",
                    "LINEAR_API_KEY not set — skipping workspace slug fetch"
                );
            }
        }
    });
}

pub use server::start_server;
