use crate::static_files;
use crate::transport::server_routes_features::{
    ai_routes, channel_routes, project_routes, scout_routes, task_routes, ui_routes,
    worktree_routes,
};
use crate::transport::{
    routes_captain, routes_captain_adopt, routes_client_logs, routes_config, routes_credentials,
    routes_sessions, routes_stats, routes_terminal, routes_workbenches, sse,
};
use crate::{ApiRouter, AppState};

pub(crate) fn public_routes() -> ApiRouter<AppState> {
    crate::api_route!(
        ApiRouter::new(),
        GET "/api/health",
        transport = Json,
        auth = Public,
        handler = routes_captain::get_health,
        res = api_types::HealthResponse
    )
}

pub(crate) fn protected_routes() -> ApiRouter<AppState> {
    task_routes()
        .merge(captain_routes())
        .merge(scout_routes())
        .merge(session_routes())
        .merge(config_routes())
        .merge(channel_routes())
        .merge(worktree_routes())
        .merge(routes_workbenches::routes())
        .merge(project_routes())
        .merge(routes_credentials::credential_routes())
        .merge(ai_routes())
        .merge(routes_terminal::routes())
        .merge(ui_routes())
        .merge(crate::api_route!(
            ApiRouter::new(),
            GET "/api/events",
            transport = Sse,
            auth = Protected,
            handler = sse::sse_events,
            event = api_types::SseEnvelope
        ))
        .merge(crate::api_route!(
            ApiRouter::new(),
            GET "/api/stats/activity",
            transport = Json,
            auth = Protected,
            handler = routes_stats::get_activity_stats,
            res = api_types::ActivityStatsResponse
        ))
        .merge(crate::api_route!(
            ApiRouter::new(),
            GET "/api/health/system",
            transport = Json,
            auth = Protected,
            handler = routes_captain::get_health_system,
            res = api_types::SystemHealthResponse
        ))
        .merge(crate::api_route!(
            ApiRouter::new(),
            GET "/api/health/telegram",
            transport = Json,
            auth = Protected,
            handler = crate::transport::routes_ui::get_telegram_health,
            res = api_types::TelegramHealth
        ))
        .merge(crate::api_route!(
            ApiRouter::new(),
            GET "/api/images/{filename}",
            transport = Static,
            auth = Protected,
            handler = static_files::get_image,
            params = api_types::ImageFilenameParams
        ))
        .merge(crate::api_route!(
            ApiRouter::new(),
            POST "/api/client-logs",
            transport = Json,
            auth = Protected,
            handler = routes_client_logs::post_client_logs,
            body = api_types::ClientLogBatchRequest,
            res = api_types::ClientLogBatchResponse
        ))
}

fn captain_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/captain/tick",
        transport = Json,
        auth = Protected,
        handler = routes_captain::post_captain_tick,
        body = api_types::TickRequest,
        res = api_types::TickResult
    );
    let router = crate::api_route!(
        router,
        POST "/api/captain/triage",
        transport = Json,
        auth = Protected,
        handler = routes_captain::post_captain_triage,
        body = api_types::TriageRequest,
        res = api_types::TriageResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/captain/stop",
        transport = Json,
        auth = Protected,
        handler = routes_captain::post_captain_stop,
        body = api_types::EmptyRequest,
        res = api_types::StopWorkersResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/captain/nudge",
        transport = Multipart,
        auth = Protected,
        handler = routes_captain::post_captain_nudge,
        body = api_types::NudgeRequest,
        res = api_types::NudgeResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/captain/adopt",
        transport = Json,
        auth = Protected,
        handler = routes_captain_adopt::post_captain_adopt,
        body = api_types::AdoptRequest,
        res = api_types::TaskCreateResponse
    );
    crate::api_route!(
        router,
        GET "/api/workers",
        transport = Json,
        auth = Protected,
        handler = routes_captain::get_workers,
        res = api_types::WorkersResponse
    )
}

fn session_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/sessions",
        transport = Json,
        auth = Protected,
        handler = routes_sessions::get_sessions,
        query = api_types::SessionsQuery,
        res = api_types::SessionsListResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/sessions/{id}/transcript",
        transport = Json,
        auth = Protected,
        handler = routes_sessions::get_session_transcript,
        params = api_types::SessionIdParams,
        res = api_types::TranscriptResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/sessions/{id}/messages",
        transport = Json,
        auth = Protected,
        handler = routes_sessions::get_session_messages,
        query = api_types::SessionMessagesQuery,
        params = api_types::SessionIdParams,
        res = api_types::SessionMessagesResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/sessions/{id}/tools",
        transport = Json,
        auth = Protected,
        handler = routes_sessions::get_session_tools,
        params = api_types::SessionIdParams,
        res = api_types::SessionToolUsageResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/sessions/{id}/cost",
        transport = Json,
        auth = Protected,
        handler = routes_sessions::get_session_cost,
        params = api_types::SessionIdParams,
        res = api_types::SessionCostResponse
    );
    crate::api_route!(
        router,
        GET "/api/sessions/{id}/stream",
        transport = Ndjson,
        auth = Protected,
        handler = routes_sessions::get_session_stream,
        query = api_types::SessionStreamQuery,
        params = api_types::SessionIdParams
    )
}

fn config_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/config",
        transport = Json,
        auth = Protected,
        handler = routes_config::get_config,
        res = api_types::MandoConfig
    );
    let router = crate::api_route!(
        router,
        PUT "/api/config",
        transport = Json,
        auth = Protected,
        handler = routes_config::put_config,
        body = api_types::MandoConfig,
        res = api_types::ConfigWriteResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/config/status",
        transport = Json,
        auth = Protected,
        handler = routes_config::get_config_status,
        res = api_types::ConfigStatusResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/config/setup",
        transport = Json,
        auth = Protected,
        handler = routes_config::post_config_setup,
        body = api_types::ConfigSetupRequest,
        res = api_types::ConfigSetupResponse
    );
    crate::api_route!(
        router,
        GET "/api/config/paths",
        transport = Json,
        auth = Protected,
        handler = routes_config::get_config_paths,
        res = api_types::ConfigPathsResponse
    )
}
