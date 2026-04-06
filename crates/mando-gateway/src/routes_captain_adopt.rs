//! /api/captain/adopt route handler and helpers.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::response::{error_response, internal_error};
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

    let central_wt_dir = mando_captain::io::git::worktrees_dir();
    let wt_name = wt_path.file_name().and_then(|n| n.to_str());
    let mut matched = config
        .captain
        .projects
        .values()
        .filter_map(|pc| {
            let project_path = mando_config::expand_tilde(&pc.path);
            if wt_path == project_path || wt_path.starts_with(&project_path) {
                return Some((pc.name.clone(), usize::MAX));
            }
            // Match worktrees in the central dir — longest prefix wins.
            if wt_path.starts_with(&central_wt_dir) {
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
        None if config.captain.projects.len() == 1 => Ok(config
            .captain
            .projects
            .values()
            .next()
            .expect("single project should exist")
            .name
            .clone()),
        None => Err(error_response(
            StatusCode::BAD_REQUEST,
            "project is required when the worktree path does not match a configured project",
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
            Some("adopt"),
        )
        .await
        .map_err(internal_error)?;

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

    Ok(Json(val))
}
