use crate::transport::{
    routes_ai, routes_channels, routes_projects, routes_scout, routes_scout_ai, routes_scout_bulk,
    routes_scout_telegraph, routes_task_router, routes_ui, routes_worktrees,
};
use crate::{ApiRouter, AppState};

pub(crate) fn task_routes() -> ApiRouter<AppState> {
    routes_task_router::task_routes()
}

pub(crate) fn scout_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/scout/items",
        transport = Json,
        auth = Protected,
        handler = routes_scout::get_scout_items,
        query = api_types::ScoutQuery,
        res = api_types::ScoutResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/items",
        transport = Json,
        auth = Protected,
        handler = routes_scout::post_scout_items,
        body = api_types::ScoutAddRequest,
        res = api_types::ScoutAddResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/items/{id}",
        transport = Json,
        auth = Protected,
        handler = routes_scout::get_scout_item,
        params = api_types::ScoutItemIdParams,
        res = api_types::ScoutItem
    );
    let router = crate::api_route!(
        router,
        PATCH "/api/scout/items/{id}",
        transport = Json,
        auth = Protected,
        handler = routes_scout::patch_scout_item,
        body = api_types::ScoutLifecycleCommandRequest,
        params = api_types::ScoutItemIdParams,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        DELETE "/api/scout/items/{id}",
        transport = Json,
        auth = Protected,
        handler = routes_scout::delete_scout_item,
        params = api_types::ScoutItemIdParams,
        res = api_types::ScoutDeleteResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/items/{id}/article",
        transport = Json,
        auth = Protected,
        handler = routes_scout::get_scout_article,
        params = api_types::ScoutItemIdParams,
        res = api_types::ScoutArticleResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/items/{id}/telegraph",
        transport = Json,
        auth = Protected,
        handler = routes_scout_telegraph::publish_telegraph,
        body = api_types::EmptyRequest,
        params = api_types::ScoutItemIdParams,
        res = api_types::TelegraphPublishResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/items/{id}/act",
        transport = Json,
        auth = Protected,
        handler = routes_scout::post_scout_act,
        body = api_types::ScoutActRequest,
        params = api_types::ScoutItemIdParams,
        res = api_types::ActResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/items/{id}/sessions",
        transport = Json,
        auth = Protected,
        handler = routes_scout::get_scout_item_sessions,
        params = api_types::ScoutItemIdParams,
        res = Vec<api_types::ScoutItemSession>
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/process",
        transport = Json,
        auth = Protected,
        handler = routes_scout::post_scout_process,
        body = api_types::ScoutProcessRequest,
        res = api_types::ProcessResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/research",
        transport = Json,
        auth = Protected,
        handler = routes_scout_ai::get_scout_research_runs,
        res = Vec<api_types::ScoutResearchRun>
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/research",
        transport = Json,
        auth = Protected,
        handler = routes_scout_ai::post_scout_research,
        body = api_types::ScoutResearchRequest,
        res = api_types::ResearchStartResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/research/{id}",
        transport = Json,
        auth = Protected,
        handler = routes_scout_ai::get_scout_research_run,
        params = api_types::ScoutResearchIdParams,
        res = api_types::ScoutResearchRun
    );
    let router = crate::api_route!(
        router,
        GET "/api/scout/research/{id}/items",
        transport = Json,
        auth = Protected,
        handler = routes_scout_ai::get_scout_research_run_items,
        params = api_types::ScoutResearchIdParams,
        res = Vec<api_types::ScoutItem>
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/ask",
        transport = Multipart,
        auth = Protected,
        handler = routes_scout_ai::post_scout_ask,
        body = api_types::ScoutAskRequest,
        res = api_types::AskResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/scout/bulk",
        transport = Json,
        auth = Protected,
        handler = routes_scout_bulk::post_scout_bulk_update,
        body = api_types::ScoutBulkCommandRequest,
        res = api_types::ScoutBulkUpdateResponse
    );
    crate::api_route!(
        router,
        POST "/api/scout/bulk-delete",
        transport = Json,
        auth = Protected,
        handler = routes_scout_bulk::post_scout_bulk_delete,
        body = api_types::ScoutBulkDeleteRequest,
        res = api_types::ScoutBulkDeleteResponse
    )
}

pub(crate) fn channel_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/channels",
        transport = Json,
        auth = Protected,
        handler = routes_channels::get_channels,
        res = api_types::ChannelsResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/channels/telegram/owner",
        transport = Json,
        auth = Protected,
        handler = routes_channels::post_telegram_owner,
        body = api_types::TelegramOwnerRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/notify",
        transport = Json,
        auth = Protected,
        handler = routes_channels::post_notify,
        body = api_types::NotifyRequest,
        res = api_types::NotifyResponse
    );
    crate::api_route!(
        router,
        POST "/api/firecrawl/scrape",
        transport = Json,
        auth = Protected,
        handler = routes_channels::post_firecrawl_scrape,
        body = api_types::FirecrawlScrapeRequest,
        res = api_types::FirecrawlScrapeResponse
    )
}

pub(crate) fn worktree_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/worktrees",
        transport = Json,
        auth = Protected,
        handler = routes_worktrees::get_worktrees,
        res = api_types::WorktreeListResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/worktrees",
        transport = Json,
        auth = Protected,
        handler = routes_worktrees::post_worktrees,
        body = api_types::CreateWorktreeRequest,
        res = api_types::CreateWorktreeResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/worktrees/prune",
        transport = Json,
        auth = Protected,
        handler = routes_worktrees::post_worktrees_prune,
        body = api_types::EmptyRequest,
        res = api_types::WorktreePruneResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/worktrees/remove",
        transport = Json,
        auth = Protected,
        handler = routes_worktrees::post_worktrees_remove,
        body = api_types::RemoveWorktreeRequest,
        res = api_types::BoolOkResponse
    );
    crate::api_route!(
        router,
        POST "/api/worktrees/cleanup",
        transport = Json,
        auth = Protected,
        handler = routes_worktrees::post_worktrees_cleanup,
        body = api_types::WorktreeCleanupRequest,
        res = api_types::WorktreeCleanupResponse
    )
}

pub(crate) fn project_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/projects",
        transport = Json,
        auth = Protected,
        handler = routes_projects::get_projects,
        res = api_types::ProjectsListResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/projects",
        transport = Json,
        auth = Protected,
        handler = routes_projects::post_projects,
        body = api_types::AddProjectRequest,
        res = api_types::ProjectUpsertResponse
    );
    let router = crate::api_route!(
        router,
        PATCH "/api/projects/{name}",
        transport = Json,
        auth = Protected,
        handler = routes_projects::patch_project,
        body = api_types::EditProjectRequest,
        params = api_types::ProjectNameParams,
        res = api_types::ProjectUpsertResponse
    );
    crate::api_route!(
        router,
        DELETE "/api/projects/{name}",
        transport = Json,
        auth = Protected,
        handler = routes_projects::delete_project,
        params = api_types::ProjectNameParams,
        res = api_types::ProjectDeleteResponse
    )
}

pub(crate) fn ai_routes() -> ApiRouter<AppState> {
    crate::api_route!(
        ApiRouter::new(),
        POST "/api/ai/parse-todos",
        transport = Json,
        auth = Protected,
        handler = routes_ai::post_parse_todos,
        body = api_types::ParseTodosRequest,
        res = api_types::ParseTodosResponse
    )
}

pub(crate) fn ui_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/ui/register",
        transport = Json,
        auth = Protected,
        handler = routes_ui::post_ui_register,
        body = api_types::UiRegisterRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/ui/quitting",
        transport = Json,
        auth = Protected,
        handler = routes_ui::post_ui_quitting,
        body = api_types::EmptyRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/ui/updating",
        transport = Json,
        auth = Protected,
        handler = routes_ui::post_ui_updating,
        body = api_types::EmptyRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/ui/launch",
        transport = Json,
        auth = Protected,
        handler = routes_ui::post_ui_launch,
        body = api_types::EmptyRequest,
        res = api_types::BoolOkResponse
    );
    crate::api_route!(
        router,
        POST "/api/ui/restart",
        transport = Json,
        auth = Protected,
        handler = routes_ui::post_ui_restart,
        body = api_types::EmptyRequest,
        res = api_types::BoolOkResponse
    )
}
