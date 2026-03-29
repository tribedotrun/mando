//! /api/projects/* route handlers — project CRUD.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Resolve a project from config by name or alias (case-insensitive).
fn resolve_project_key(
    config: &mando_config::Config,
    identifier: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let id_lower = identifier.to_lowercase();
    for (k, v) in &config.captain.projects {
        if v.name.to_lowercase() == id_lower
            || v.aliases.iter().any(|a| a.to_lowercase() == id_lower)
        {
            return Ok(k.clone());
        }
    }
    Err(error_response(
        StatusCode::NOT_FOUND,
        &format!("project not found: {identifier}"),
    ))
}

/// Save config to disk and hot-reload into daemon state.
async fn save_and_reload(
    state: &AppState,
    config: &mando_config::Config,
) -> Result<(), (StatusCode, Json<Value>)> {
    mando_config::save_config(config, None).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("save failed: {e}"),
        )
    })?;

    state.config.store(Arc::new(config.clone()));
    Ok(())
}

/// GET /api/projects — list all projects.
pub(crate) async fn get_projects(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let projects: Vec<Value> = config
        .captain
        .projects
        .iter()
        .map(|(key, pc)| {
            json!({
                "key": key,
                "name": pc.name,
                "path": pc.path,
                "githubRepo": pc.github_repo,
                "aliases": pc.aliases,
            })
        })
        .collect();

    Json(json!({ "projects": projects }))
}

#[derive(Deserialize)]
pub(crate) struct AddProjectBody {
    pub name: Option<String>,
    pub path: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// POST /api/projects — add a new project.
pub(crate) async fn post_projects(
    State(state): State<AppState>,
    Json(body): Json<AddProjectBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _write_guard = state.config_write_mu.lock().await;
    let mut config = (*state.config.load_full()).clone();
    let abs_path = mando_config::expand_tilde(&body.path);

    // Validate path exists and is a directory.
    if !abs_path.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("directory does not exist: {}", abs_path.display()),
        ));
    }

    let abs_path_str = abs_path.to_string_lossy().into_owned();

    // Canonical key = absolute path.
    let key = abs_path_str.clone();

    // Default name to folder basename if not provided.
    let name = match &body.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string(),
    };

    // Check duplicate key or name.
    if config.captain.projects.contains_key(&key) {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!("project already exists at path: {key}"),
        ));
    }
    let name_lower = name.to_lowercase();
    for v in config.captain.projects.values() {
        if v.name.to_lowercase() == name_lower {
            return Err(error_response(
                StatusCode::CONFLICT,
                &format!("project name already exists: {}", v.name),
            ));
        }
    }

    // Auto-detect GitHub repo.
    let github_repo = mando_config::detect_github_repo(&abs_path_str);

    let pc = mando_config::settings::ProjectConfig {
        name: name.clone(),
        path: abs_path_str.clone(),
        github_repo: github_repo.clone(),
        aliases: body.aliases,
        ..Default::default()
    };

    config.captain.projects.insert(key, pc);
    save_and_reload(&state, &config).await?;

    Ok(Json(json!({
        "ok": true,
        "name": name,
        "path": abs_path_str,
        "githubRepo": github_repo,
    })))
}

#[derive(Deserialize)]
pub(crate) struct EditProjectBody {
    pub rename: Option<String>,
    pub github_repo: Option<String>,
    pub clear_github_repo: Option<bool>,
    pub aliases: Option<Vec<String>>,
    pub preamble: Option<String>,
    pub check_command: Option<String>,
}

/// PATCH /api/projects/{name} — edit a project.
pub(crate) async fn patch_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<EditProjectBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _write_guard = state.config_write_mu.lock().await;
    let mut config = (*state.config.load_full()).clone();
    let key = resolve_project_key(&config, &name)?;

    // Check rename uniqueness.
    if let Some(ref new_name) = body.rename {
        let new_lower = new_name.to_lowercase();
        for (k, v) in &config.captain.projects {
            if *k != key && v.name.to_lowercase() == new_lower {
                return Err(error_response(
                    StatusCode::CONFLICT,
                    &format!("project name already exists: {}", v.name),
                ));
            }
        }
    }

    let pc = config
        .captain
        .projects
        .get_mut(&key)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "project vanished"))?;

    if let Some(new_name) = body.rename {
        pc.name = new_name;
    }
    if body.clear_github_repo == Some(true) {
        pc.github_repo = None;
    } else if let Some(repo) = body.github_repo {
        pc.github_repo = Some(repo);
    }
    if let Some(aliases) = body.aliases {
        pc.aliases = aliases;
    }
    if let Some(preamble) = body.preamble {
        pc.worker_preamble = preamble;
    }
    if let Some(check_cmd) = body.check_command {
        pc.check_command = check_cmd;
    }

    save_and_reload(&state, &config).await?;
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /api/projects/{name} — remove a project.
pub(crate) async fn delete_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _write_guard = state.config_write_mu.lock().await;
    let mut config = (*state.config.load_full()).clone();
    let key = resolve_project_key(&config, &name)?;
    config.captain.projects.remove(&key);
    save_and_reload(&state, &config).await?;
    Ok(Json(json!({ "ok": true })))
}
