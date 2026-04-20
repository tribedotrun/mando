//! /api/worktrees/* route handlers.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// POST /api/worktrees — create a worktree.
#[crate::instrument_api(method = "POST", path = "/api/worktrees")]
pub(crate) async fn post_worktrees(
    State(state): State<AppState>,
    Json(body): Json<api_types::CreateWorktreeRequest>,
) -> Result<Json<api_types::CreateWorktreeResponse>, ApiError> {
    let project = body
        .project
        .as_deref()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "project name required"))?;

    match state
        .captain
        .create_worktree(project, body.name.as_deref())
        .await
        .map_err(|e| internal_error(e, "failed to create worktree"))?
    {
        captain::CreateWorktreeOutcome::Created(created) => {
            Ok(Json(api_types::CreateWorktreeResponse {
                ok: true,
                path: created.path,
                branch: created.branch,
                project: created.project,
                workbench_id: created.workbench_id,
            }))
        }
        captain::CreateWorktreeOutcome::ProjectNotFound(project) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("project not found: {project}"),
        )),
        captain::CreateWorktreeOutcome::Conflict(message) => {
            Err(error_response(StatusCode::CONFLICT, &message))
        }
    }
}

/// GET /api/worktrees — list all worktrees across projects.
#[crate::instrument_api(method = "GET", path = "/api/worktrees")]
pub(crate) async fn get_worktrees(
    State(state): State<AppState>,
) -> Result<Json<api_types::WorktreeListResponse>, ApiError> {
    let worktrees = state
        .captain
        .list_worktrees()
        .await
        .map_err(|e| internal_error(e, "failed to load projects"))?;
    let worktrees = worktrees
        .into_iter()
        .map(|entry| api_types::WorktreeListItem {
            project: entry.project,
            path: entry.path,
        })
        .collect();
    Ok(Json(api_types::WorktreeListResponse { worktrees }))
}

/// POST /api/worktrees/prune — prune stale worktrees for all projects.
#[crate::instrument_api(method = "POST", path = "/api/worktrees/prune")]
pub(crate) async fn post_worktrees_prune(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::WorktreePruneResponse>, ApiError> {
    let pruned = state
        .captain
        .prune_worktrees()
        .await
        .map_err(|e| internal_error(e, "failed to load projects"))?;
    Ok(Json(api_types::WorktreePruneResponse { ok: true, pruned }))
}

/// POST /api/worktrees/remove — remove a specific worktree by full path.
#[crate::instrument_api(method = "POST", path = "/api/worktrees/remove")]
pub(crate) async fn post_worktrees_remove(
    State(state): State<AppState>,
    Json(body): Json<api_types::RemoveWorktreeRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    match state
        .captain
        .remove_worktree(Path::new(&body.path))
        .await
        .map_err(|e| internal_error(e, "failed to remove worktree"))?
    {
        captain::RemoveWorktreeOutcome::Removed => Ok(Json(api_types::BoolOkResponse { ok: true })),
        captain::RemoveWorktreeOutcome::NotFound => Err(error_response(
            StatusCode::NOT_FOUND,
            "no project owns this worktree path",
        )),
    }
}

/// POST /api/worktrees/cleanup — find and optionally remove orphan worktree dirs.
#[crate::instrument_api(method = "POST", path = "/api/worktrees/cleanup")]
pub(crate) async fn post_worktrees_cleanup(
    State(state): State<AppState>,
    Json(body): Json<api_types::WorktreeCleanupRequest>,
) -> Result<Json<api_types::WorktreeCleanupResponse>, ApiError> {
    let report = state
        .captain
        .cleanup_worktrees(body.dry_run)
        .await
        .map_err(|e| internal_error(e, "failed to load projects"))?;
    Ok(Json(api_types::WorktreeCleanupResponse {
        ok: true,
        orphans: report.orphans,
        removed: report.removed,
        prune_errors: report
            .prune_errors
            .into_iter()
            .map(|err| api_types::WorktreePruneError {
                project: err.project,
                error: err.error,
            })
            .collect(),
    }))
}
