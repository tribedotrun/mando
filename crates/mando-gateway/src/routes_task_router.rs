//! Task route tree -- extracted to respect file length limits.

use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use crate::{
    routes_artifacts, routes_clarifier, routes_task_actions, routes_task_advisor, routes_task_ask,
    routes_task_detail, routes_tasks, AppState,
};

pub(crate) fn task_routes() -> Router<AppState> {
    Router::new()
        .merge(task_detail_routes())
        .merge(artifact_routes())
        .route("/api/tasks", get(routes_tasks::get_tasks))
        .route("/api/tasks", delete(routes_tasks::delete_task_items))
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
            "/api/tasks/ask/reopen",
            post(routes_task_ask::post_task_ask_reopen),
        )
        .route(
            "/api/tasks/retry",
            post(routes_task_actions::post_task_retry),
        )
        .route(
            "/api/tasks/resume-rate-limited",
            post(routes_task_actions::post_task_resume_rate_limited),
        )
        .route(
            "/api/tasks/{id}/clarify",
            post(routes_clarifier::post_task_clarify),
        )
}

fn task_detail_routes() -> Router<AppState> {
    Router::new()
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
        .route(
            "/api/tasks/{id}/artifacts",
            get(routes_task_detail::get_task_artifacts),
        )
        .route(
            "/api/tasks/{id}/feed",
            get(routes_task_detail::get_task_feed),
        )
        .route(
            "/api/tasks/{id}/advisor",
            post(routes_task_advisor::post_task_advisor),
        )
}

fn artifact_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tasks/{id}/evidence",
            post(routes_artifacts::post_task_evidence),
        )
        .route(
            "/api/tasks/{id}/summary",
            post(routes_artifacts::post_task_summary),
        )
        .route(
            "/api/artifacts/{id}/media/{index}",
            get(routes_artifacts::get_artifact_media),
        )
        .route(
            "/api/artifacts/{id}/media",
            put(routes_artifacts::put_artifact_media),
        )
}
