//! Task route tree -- extracted to respect file length limits.

use crate::{
    routes_artifacts, routes_clarifier, routes_task_actions, routes_task_advisor, routes_task_ask,
    routes_task_detail, routes_tasks, ApiRouter, AppState,
};

pub(crate) fn task_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new()
        .merge(task_detail_routes())
        .merge(artifact_routes());
    let router = crate::api_route!(
        router,
        GET "/api/tasks",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::get_tasks,
        query = api_types::TaskListQuery,
        res = api_types::TaskListResponse
    );
    let router = crate::api_route!(
        router,
        DELETE "/api/tasks",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::delete_task_items,
        body = api_types::TaskDeleteRequest,
        res = api_types::DeleteTasksResponse
    );
    let router = crate::api_route!(
        router,
        PATCH "/api/tasks/{id}",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::patch_task_item,
        body = api_types::TaskPatchRequest,
        params = api_types::TaskIdParams,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/add",
        transport = Multipart,
        auth = Protected,
        handler = routes_tasks::post_task_add,
        body = api_types::TaskAddRequest,
        res = api_types::TaskItem
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/bulk",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::post_task_bulk,
        body = api_types::TaskBulkUpdateRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/delete",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::post_task_delete,
        body = api_types::TaskDeleteRequest,
        res = api_types::DeleteTasksResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/merge",
        transport = Json,
        auth = Protected,
        handler = routes_tasks::post_task_merge,
        body = api_types::MergeRequest,
        res = api_types::MergeResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/accept",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_accept,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/queue",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_queue,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/cancel",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_cancel,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/reopen",
        transport = Multipart,
        auth = Protected,
        handler = routes_task_actions::post_task_reopen,
        body = api_types::TaskFeedbackRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/rework",
        transport = Multipart,
        auth = Protected,
        handler = routes_task_actions::post_task_rework,
        body = api_types::TaskFeedbackRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/handoff",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_handoff,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/ask",
        transport = Multipart,
        auth = Protected,
        handler = routes_task_ask::post_task_ask,
        body = api_types::TaskAskRequest,
        res = api_types::AskResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/ask/end",
        transport = Json,
        auth = Protected,
        handler = routes_task_ask::post_task_ask_end,
        body = api_types::TaskIdRequest,
        res = api_types::AskEndResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/ask/reopen",
        transport = Json,
        auth = Protected,
        handler = routes_task_ask::post_task_ask_reopen,
        body = api_types::TaskIdRequest,
        res = api_types::AskReopenResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/retry",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_retry,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/resume-rate-limited",
        transport = Json,
        auth = Protected,
        handler = routes_task_actions::post_task_resume_rate_limited,
        body = api_types::TaskIdRequest,
        res = api_types::BoolOkResponse
    );
    crate::api_route!(
        router,
        POST "/api/tasks/{id}/clarify",
        transport = Multipart,
        auth = Protected,
        handler = routes_clarifier::post_task_clarify,
        body = api_types::ClarifyRequest,
        params = api_types::TaskIdParams,
        res = api_types::ClarifyResponse
    )
}

fn task_detail_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/history",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_history,
        params = api_types::TaskIdParams,
        res = api_types::AskHistoryResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/timeline",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_timeline,
        params = api_types::TaskIdParams,
        res = api_types::TimelineResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/pr-summary",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_pr_summary,
        params = api_types::TaskIdParams,
        res = api_types::PrSummaryResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/sessions",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_sessions,
        query = api_types::SessionsQuery,
        params = api_types::TaskIdParams,
        res = api_types::ItemSessionsResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/artifacts",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_artifacts,
        params = api_types::TaskIdParams,
        res = api_types::ArtifactsResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/tasks/{id}/feed",
        transport = Json,
        auth = Protected,
        handler = routes_task_detail::get_task_feed,
        params = api_types::TaskIdParams,
        res = api_types::FeedResponse
    );
    crate::api_route!(
        router,
        POST "/api/tasks/{id}/advisor",
        transport = Json,
        auth = Protected,
        handler = routes_task_advisor::post_task_advisor,
        body = api_types::AdvisorRequest,
        params = api_types::TaskIdParams,
        res = api_types::AdvisorResponse
    )
}

fn artifact_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/tasks/{id}/evidence",
        transport = Json,
        auth = Protected,
        handler = routes_artifacts::post_task_evidence,
        body = api_types::TaskEvidenceRequest,
        params = api_types::TaskIdParams,
        res = api_types::TaskEvidenceResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/tasks/{id}/summary",
        transport = Json,
        auth = Protected,
        handler = routes_artifacts::post_task_summary,
        body = api_types::TaskSummaryRequest,
        params = api_types::TaskIdParams,
        res = api_types::TaskSummaryResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/artifacts/{id}/media/{index}",
        transport = Static,
        auth = Protected,
        handler = routes_artifacts::get_artifact_media,
        params = api_types::ArtifactMediaParams
    );
    crate::api_route!(
        router,
        PUT "/api/artifacts/{id}/media",
        transport = Json,
        auth = Protected,
        handler = routes_artifacts::put_artifact_media,
        body = api_types::ArtifactMediaUpdateRequest,
        params = api_types::ArtifactIdParams,
        res = api_types::BoolOkResponse
    )
}
