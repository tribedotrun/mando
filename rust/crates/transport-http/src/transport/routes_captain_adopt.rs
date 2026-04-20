//! /api/captain/adopt route handler and helpers.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use captain::UpdateTaskInput;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

fn resolve_adopt_project(
    config: &settings::config::Config,
    requested: Option<&str>,
    wt_path: &Path,
    central_wt_dir: &Path,
) -> Result<String, ApiError> {
    if let Some(project) = requested {
        return settings::config::resolve_project_config(Some(project), config)
            .map(|(_, pc)| pc.name.clone())
            .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "unknown project"));
    }

    let wt_name = wt_path.file_name().and_then(|n| n.to_str());
    let mut matched = config
        .captain
        .projects
        .values()
        .filter_map(|pc| {
            let project_path = global_infra::paths::expand_tilde(&pc.path);
            if wt_path == project_path || wt_path.starts_with(&project_path) {
                return Some((pc.name.clone(), usize::MAX));
            }
            // Match worktrees in the central dir — longest prefix wins.
            if wt_path.starts_with(central_wt_dir) {
                let repo_name = project_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let prefix = format!("{repo_name}-");
                if let Some(name) = wt_name {
                    if name.starts_with(&prefix) {
                        return Some((pc.name.clone(), prefix.len()));
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>();

    // Pick the longest-prefix match (most specific project).
    matched.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    matched.dedup_by(|a, b| a.0 == b.0);

    match matched.first() {
        Some((project, _)) => Ok(project.clone()),
        None if config.captain.projects.len() == 1 => match config.captain.projects.values().next()
        {
            Some(p) => Ok(p.name.clone()),
            None => Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "projects map length == 1 but values().next() returned None",
            )),
        },
        None => Err(error_response(
            StatusCode::BAD_REQUEST,
            "project is required when the worktree path does not match a configured project",
        )),
    }
}

async fn detect_checked_out_branch(state: &AppState, wt_path: &Path) -> Result<String, ApiError> {
    let branch = state
        .captain
        .current_worktree_branch(wt_path)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_REQUEST,
                &format!("failed to inspect git branch: {e}"),
            )
        })?;

    if !branch.is_empty() && branch != "HEAD" {
        return Ok(branch);
    }

    Err(error_response(
        StatusCode::BAD_REQUEST,
        "path must point to a git worktree with a checked-out branch",
    ))
}

/// POST /api/captain/adopt
#[crate::instrument_api(method = "POST", path = "/api/captain/adopt")]
pub(crate) async fn post_captain_adopt(
    State(state): State<AppState>,
    Json(body): Json<api_types::AdoptRequest>,
) -> Result<Json<api_types::TaskCreateResponse>, ApiError> {
    let config = state.settings.load_config();
    let wt_path = Path::new(&body.worktree_path);
    if !wt_path.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "path must point to an existing worktree directory",
        ));
    }

    let central_wt_dir = state.captain.worktrees_dir();
    let project_name =
        resolve_adopt_project(&config, body.project.as_deref(), wt_path, &central_wt_dir)?;
    let branch = detect_checked_out_branch(&state, wt_path).await?;

    let brief_dir = wt_path.join(".ai").join("briefs");
    tokio::fs::create_dir_all(&brief_dir).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to create adopt brief directory: {e}"),
        )
    })?;
    let note_text = body
        .note
        .as_deref()
        .unwrap_or("Continue from current state. Run tests, fix failures, create PR.");
    let brief = format!(
        "# Adopt Handoff\n\nBranch: {branch}\nTitle: {}\n\n{note_text}\n",
        body.title
    );
    tokio::fs::write(brief_dir.join("adopt-handoff.md"), &brief)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to write adopt brief: {e}"),
            )
        })?;

    let wt_display = wt_path.display().to_string();
    let created = {
        let ctx = match &body.note {
            Some(note) => format!("Adopted from worktree: {wt_display}\n\nNote: {note}"),
            None => format!("Adopted from worktree: {wt_display}"),
        };
        let created = state
            .captain
            .add_task_with_context(
                &body.title,
                Some(project_name.as_str()),
                Some(&ctx),
                Some("adopt"),
            )
            .await
            .map_err(|e| internal_error(e, "failed to create task for adoption"))?;

        let id = created.id;

        state
            .captain
            .update_task(
                id,
                UpdateTaskInput {
                    plan: Some(Some(".ai/briefs/adopt-handoff.md".into())),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("item created but update failed: {e}"),
                )
            })?;
        state
            .captain
            .queue_item(id, "captain_adopt")
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("item created but queue failed: {e}"),
                )
            })?;
        created
    };

    Ok(Json(api_types::TaskCreateResponse {
        id: created.id,
        title: created.title,
    }))
}
