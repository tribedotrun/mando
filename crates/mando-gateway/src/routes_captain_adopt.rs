//! /api/captain/adopt route handler and helpers.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::response::error_response;
use crate::AppState;

fn resolve_adopt_project(
    config: &mando_config::settings::Config,
    requested: Option<&str>,
    wt_path: &Path,
) -> Result<String, (StatusCode, Json<Value>)> {
    if let Some(project) = requested {
        return mando_config::resolve_project_config(Some(project), config)
            .map(|(_, pc)| pc.name.clone())
            .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "unknown project"));
    }

    let mut matched = config
        .captain
        .projects
        .values()
        .filter_map(|pc| {
            let project_path = mando_config::expand_tilde(&pc.path);
            let worktrees_dir = project_path
                .parent()
                .unwrap_or(&project_path)
                .join("worktrees");
            if wt_path == project_path
                || wt_path.starts_with(&project_path)
                || wt_path.starts_with(&worktrees_dir)
            {
                Some(pc.name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    matched.sort();
    matched.dedup();

    match matched.as_slice() {
        [project] => Ok(project.clone()),
        [] if config.captain.projects.len() == 1 => Ok(config
            .captain
            .projects
            .values()
            .next()
            .expect("single project should exist")
            .name
            .clone()),
        [] => Err(error_response(
            StatusCode::BAD_REQUEST,
            "project is required when the worktree path does not match a configured project",
        )),
        _ => Err(error_response(
            StatusCode::BAD_REQUEST,
            "multiple projects match this worktree path; choose a project explicitly",
        )),
    }
}

async fn detect_checked_out_branch(wt_path: &Path) -> Result<String, (StatusCode, Json<Value>)> {
    for args in [
        ["branch", "--show-current"].as_slice(),
        ["rev-parse", "--abbrev-ref", "HEAD"].as_slice(),
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(wt_path)
            .output()
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("failed to inspect git branch: {e}"),
                )
            })?;

        if !output.status.success() {
            continue;
        }

        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() && branch != "HEAD" {
            return Ok(branch);
        }
    }

    Err(error_response(
        StatusCode::BAD_REQUEST,
        "path must point to a git worktree with a checked-out branch",
    ))
}

#[derive(Deserialize)]
pub(crate) struct AdoptBody {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// POST /api/captain/adopt
pub(crate) async fn post_captain_adopt(
    State(state): State<AppState>,
    Json(body): Json<AdoptBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let wt_path = Path::new(&body.path);
    if !wt_path.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "path must point to an existing worktree directory",
        ));
    }

    let project_name = resolve_adopt_project(&config, body.project.as_deref(), wt_path)?;
    let detected_branch = detect_checked_out_branch(wt_path).await?;
    let branch = match body
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(requested) if requested != detected_branch => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "supplied branch does not match the checked-out branch",
            ))
        }
        _ => detected_branch,
    };

    let brief_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&brief_dir).map_err(|e| {
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
    std::fs::write(brief_dir.join("adopt-handoff.md"), &brief).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to write adopt brief: {e}"),
        )
    })?;

    let wt_display = wt_path.display().to_string();
    let val = {
        let store = state.task_store.read().await;
        let ctx = match &body.note {
            Some(note) => format!("Adopted from worktree: {wt_display}\n\nNote: {note}"),
            None => format!("Adopted from worktree: {wt_display}"),
        };
        let val = mando_captain::runtime::dashboard::add_task_with_context(
            &config,
            &store,
            &body.title,
            Some(project_name.as_str()),
            Some(&ctx),
        )
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

        let id = val["id"].as_i64().ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task created but returned no id",
            )
        })?;

        let updates = json!({
            "worktree": wt_display,
            "branch": branch,
            "status": "queued",
            "plan": ".ai/briefs/adopt-handoff.md"
        });
        mando_captain::runtime::dashboard::update_task(&store, id, &updates)
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("item created but update failed: {e}"),
                )
            })?;
        val
    };

    // Adopted worktrees typically already have context — only create a Linear
    // issue if one wasn't already associated via the original task.
    let item_id = val["id"].as_i64();
    if let Some(id) = item_id {
        let item = {
            let store = state.task_store.read().await;
            store.find_by_id(id).await.unwrap_or(None)
        };
        let needs_linear = item.as_ref().is_none_or(|i| i.linear_id.is_none());
        if needs_linear {
            crate::routes_tasks::create_linear_issue_for_new_item(&state, &config, id).await;
        }
    }

    Ok(Json(val))
}
