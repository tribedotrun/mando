//! /api/projects/* route handlers — project CRUD.
//!
//! Source of truth: `projects` DB table. After every DB write the in-memory
//! config is reloaded from the DB and persisted to config.json so that the
//! rest of the system (captain, scouts, etc.) sees the change immediately.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use mando_db::queries::projects as db;

use crate::response::{error_response, internal_error};
use crate::AppState;

/// Resolve a project from the DB by name or alias.
/// Returns the full `ProjectRow` or a 404 HTTP error.
async fn resolve_project(
    state: &AppState,
    identifier: &str,
) -> Result<db::ProjectRow, (StatusCode, Json<Value>)> {
    db::resolve(state.db.pool(), identifier)
        .await
        .map_err(|e| internal_error(e, "failed to resolve project"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("project not found: {identifier}"),
            )
        })
}

/// After a DB write, reload all projects from the DB into the in-memory
/// config and persist to config.json. This keeps every subsystem in sync.
async fn reload_config_from_db(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    let mut cfg = state.config.load_full().as_ref().clone();
    db::load_into_config(state.db.pool(), &mut cfg)
        .await
        .map_err(|e| internal_error(e, "failed to load project config from DB"))?;
    state
        .config_manager
        .replace(cfg)
        .await
        .map_err(|e| internal_error(e, "failed to persist config"))?;
    Ok(())
}

/// GET /api/projects — list all projects.
pub(crate) async fn get_projects(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = db::list(state.db.pool())
        .await
        .map_err(|e| internal_error(e, "failed to load projects"))?;
    let projects: Vec<Value> = rows
        .iter()
        .map(|row| {
            let aliases: Vec<String> = serde_json::from_str(&row.aliases).unwrap_or_default();
            let hooks: serde_json::Map<String, Value> =
                serde_json::from_str(&row.hooks).unwrap_or_default();
            json!({
                "key": row.path,
                "name": row.name,
                "path": row.path,
                "githubRepo": row.github_repo,
                "logo": row.logo,
                "aliases": aliases,
                "hooks": hooks,
                "workerPreamble": row.worker_preamble,
                "scoutSummary": row.scout_summary,
                "checkCommand": row.check_command,
            })
        })
        .collect();

    Ok(Json(json!({ "projects": projects })))
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
    let abs_path = mando_config::expand_tilde(&body.path);

    // Validate path exists and is a directory.
    if !abs_path.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("directory does not exist: {}", abs_path.display()),
        ));
    }

    let abs_path_str = abs_path.to_string_lossy().into_owned();

    // Default name to folder basename if not provided.
    let name = match &body.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string(),
    };

    // Reject if directory is not a git repo.
    let is_git = tokio::fs::try_exists(abs_path.join(".git"))
        .await
        .unwrap_or(false)
        || tokio::process::Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&abs_path)
            .output()
            .await
            .is_ok_and(|o| o.status.success());
    if !is_git {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!(
                "not a git repository: {}. Initialize git before adding as a project.",
                abs_path.display()
            ),
        ));
    }

    // Auto-detect GitHub repo — reject if not found.
    let github_repo = mando_config::detect_github_repo(&abs_path_str);
    if github_repo.is_none() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!(
                "no GitHub remote detected in {}. Add a GitHub remote (git remote add origin ...) before adding as a project.",
                abs_path.display()
            ),
        ));
    }

    // Check for duplicate name or path in DB.
    let pool = state.db.pool();
    if let Some(_existing) = db::find_by_name(pool, &name)
        .await
        .map_err(|e| internal_error(e, "failed to check project name"))?
    {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!("project name already exists: {name}"),
        ));
    }
    if let Some(_existing) = db::find_by_path(pool, &abs_path_str)
        .await
        .map_err(|e| internal_error(e, "failed to check project path"))?
    {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!("project already exists at path: {abs_path_str}"),
        ));
    }

    // Auto-generate scout summary and logo.
    let scout_summary = detect_project_summary(&abs_path).await;
    let logo = detect_project_logo(&abs_path, &name);

    let pc = mando_config::settings::ProjectConfig {
        name: name.clone(),
        path: abs_path_str.clone(),
        github_repo: github_repo.clone(),
        logo: logo.clone(),
        aliases: body.aliases,
        scout_summary,
        ..Default::default()
    };

    let row = db::config_to_row(&pc);
    db::upsert_full(pool, &row)
        .await
        .map_err(|e| internal_error(e, "failed to save project"))?;
    reload_config_from_db(&state).await?;
    state.bus.send(mando_types::BusEvent::Config, None);

    Ok(Json(json!({
        "ok": true,
        "name": name,
        "path": abs_path_str,
        "githubRepo": github_repo,
        "logo": logo,
    })))
}

#[derive(Deserialize)]
pub(crate) struct EditProjectBody {
    pub rename: Option<String>,
    pub github_repo: Option<String>,
    pub clear_github_repo: Option<bool>,
    pub aliases: Option<Vec<String>>,
    pub hooks: Option<std::collections::HashMap<String, String>>,
    pub preamble: Option<String>,
    pub check_command: Option<String>,
    pub scout_summary: Option<String>,
    pub redetect_logo: Option<bool>,
}

/// PATCH /api/projects/{name} — edit a project.
pub(crate) async fn patch_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<EditProjectBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut row = resolve_project(&state, &name).await?;
    let pool = state.db.pool();

    // Check rename uniqueness.
    if let Some(ref new_name) = body.rename {
        if let Some(existing) = db::find_by_name(pool, new_name)
            .await
            .map_err(|e| internal_error(e, "failed to check project name"))?
        {
            if existing.id != row.id {
                return Err(error_response(
                    StatusCode::CONFLICT,
                    &format!("project name already exists: {new_name}"),
                ));
            }
        }
        row.name = new_name.clone();
    }

    if body.clear_github_repo == Some(true) {
        row.github_repo = None;
    } else if let Some(ref repo) = body.github_repo {
        row.github_repo = Some(repo.clone());
    }
    if let Some(ref aliases) = body.aliases {
        row.aliases = serde_json::to_string(aliases).unwrap_or_else(|_| "[]".into());
    }
    if let Some(ref hooks) = body.hooks {
        row.hooks = serde_json::to_string(hooks).unwrap_or_else(|_| "{}".into());
    }
    if let Some(ref preamble) = body.preamble {
        row.worker_preamble = preamble.clone();
    }
    if let Some(ref check_cmd) = body.check_command {
        row.check_command = check_cmd.clone();
    }
    if let Some(ref summary) = body.scout_summary {
        row.scout_summary = summary.clone();
    }
    if body.redetect_logo == Some(true) {
        let project_path = std::path::Path::new(&row.path);
        row.logo = detect_project_logo(project_path, &row.name);
    }

    let logo = row.logo.clone();
    db::update(pool, row.id, &row)
        .await
        .map_err(|e| internal_error(e, "failed to update project"))?;
    reload_config_from_db(&state).await?;
    state.bus.send(mando_types::BusEvent::Config, None);

    Ok(Json(json!({ "ok": true, "logo": logo })))
}

/// DELETE /api/projects/{name} — remove a project and cascade-delete its tasks.
pub(crate) async fn delete_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = resolve_project(&state, &name).await?;
    let aliases: Vec<String> = serde_json::from_str(&row.aliases).unwrap_or_default();

    // Collect all identifiers tasks might use to reference this project.
    let mut identifiers = vec![row.path.clone(), row.name.clone()];
    identifiers.extend(aliases);

    // Cascade-delete tasks belonging to this project.
    let config = state.config.load_full();
    let store = state.task_store.read().await;
    let all_tasks = store
        .load_all_with_archived()
        .await
        .map_err(|e| internal_error(e, "failed to load tasks"))?;
    let task_ids: Vec<i64> = all_tasks
        .iter()
        .filter(|t| {
            identifiers
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&t.project))
        })
        .map(|t| t.id)
        .collect();
    let deleted_tasks = task_ids.len();

    if !task_ids.is_empty() {
        let opts = mando_captain::io::task_cleanup::CleanupOptions {
            close_pr: false,
            force: true,
        };
        mando_captain::runtime::dashboard::delete_tasks(&config, &store, &task_ids, &opts)
            .await
            .map_err(|e| internal_error(e, "failed to delete project tasks"))?;
        for tid in &task_ids {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "deleted", "id": tid})),
            );
        }
    }

    db::delete(state.db.pool(), row.id)
        .await
        .map_err(|e| internal_error(e, "failed to delete project"))?;
    reload_config_from_db(&state).await?;
    state.bus.send(mando_types::BusEvent::Config, None);

    Ok(Json(json!({ "ok": true, "deleted_tasks": deleted_tasks })))
}

/// Read a file, pass its contents to an extractor, and return the first
/// non-empty result. Used to chain multiple source files when auto-detecting a
/// project summary.
async fn try_source(
    path: std::path::PathBuf,
    extract: impl FnOnce(&str) -> Option<String>,
) -> Option<String> {
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    extract(&content).filter(|s| !s.is_empty())
}

fn detect_project_logo(project_path: &std::path::Path, project_name: &str) -> Option<String> {
    mando_config::detect_project_logo(project_path, project_name)
}
/// Auto-detect a project summary from Cargo.toml, package.json, or README.
async fn detect_project_summary(project_path: &std::path::Path) -> String {
    // Cargo.toml [package].description.
    if let Some(desc) = try_source(project_path.join("Cargo.toml"), |content| {
        content.parse::<toml::Table>().ok().and_then(|toml| {
            toml.get("package")
                .and_then(|p| p.get("description"))
                .and_then(|d| d.as_str())
                .map(|s| s.trim().to_string())
        })
    })
    .await
    {
        return desc;
    }

    // package.json description.
    if let Some(desc) = try_source(project_path.join("package.json"), |content| {
        serde_json::from_str::<Value>(content).ok().and_then(|pkg| {
            pkg.get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.trim().to_string())
        })
    })
    .await
    {
        return desc;
    }

    // First meaningful line of README.md (skip headings and blank lines).
    if let Some(desc) = try_source(project_path.join("README.md"), |content| {
        content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("!["))
            .map(|line| {
                let summary: String = line.chars().take(200).collect();
                if summary.len() < line.len() {
                    format!("{summary}…")
                } else {
                    summary
                }
            })
    })
    .await
    {
        return desc;
    }

    String::new()
}
