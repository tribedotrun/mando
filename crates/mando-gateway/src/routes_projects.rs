//! /api/projects/* route handlers — project CRUD.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Conflict error raised when a project name or path already exists.
#[derive(Debug)]
struct ProjectConflict(String);

impl std::fmt::Display for ProjectConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProjectConflict {}

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

/// Map an `anyhow::Error` from `config_manager.update()` into an HTTP error.
/// [`ProjectConflict`] errors become 409; everything else is 500.
fn update_err(e: anyhow::Error) -> (StatusCode, Json<Value>) {
    if e.downcast_ref::<ProjectConflict>().is_some() {
        return error_response(StatusCode::CONFLICT, &format!("{e}"));
    }
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &format!("save failed: {e}"),
    )
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
                "logo": pc.logo,
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
    let abs_path = mando_config::expand_tilde(&body.path);

    // Validate path exists and is a directory.
    if !abs_path.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("directory does not exist: {}", abs_path.display()),
        ));
    }

    let abs_path_str = abs_path.to_string_lossy().into_owned();
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

    // Auto-generate scout summary and logo (async I/O, before lock).
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

    // Atomically check duplicates and insert under write lock.
    state
        .config_manager
        .update(|cfg| {
            if cfg.captain.projects.contains_key(&key) {
                return Err(
                    ProjectConflict(format!("project already exists at path: {key}")).into(),
                );
            }
            let name_lower = name.to_lowercase();
            for v in cfg.captain.projects.values() {
                if v.name.to_lowercase() == name_lower {
                    return Err(ProjectConflict(format!(
                        "project name already exists: {}",
                        v.name
                    ))
                    .into());
                }
            }
            cfg.captain.projects.insert(key.clone(), pc.clone());
            Ok(())
        })
        .await
        .map_err(update_err)?;

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
    // Early 404 check (racy, but avoids holding the lock for the common case).
    let config = state.config.load_full();
    let _ = resolve_project_key(&config, &name)?;

    let logo = std::sync::Arc::new(std::sync::Mutex::new(None::<Option<String>>));
    let logo_out = logo.clone();

    state
        .config_manager
        .update(|cfg| {
            let key = resolve_project_key(cfg, &name)
                .map_err(|(_, j)| anyhow::anyhow!("{}", j.0["error"]))?;

            if let Some(ref new_name) = body.rename {
                let new_lower = new_name.to_lowercase();
                for (k, v) in &cfg.captain.projects {
                    if *k != key && v.name.to_lowercase() == new_lower {
                        return Err(ProjectConflict(format!(
                            "project name already exists: {}",
                            v.name
                        ))
                        .into());
                    }
                }
            }

            let pc = cfg
                .captain
                .projects
                .get_mut(&key)
                .ok_or_else(|| anyhow::anyhow!("project vanished"))?;

            if let Some(ref new_name) = body.rename {
                pc.name = new_name.clone();
            }
            if body.clear_github_repo == Some(true) {
                pc.github_repo = None;
            } else if let Some(ref repo) = body.github_repo {
                pc.github_repo = Some(repo.clone());
            }
            if let Some(ref aliases) = body.aliases {
                pc.aliases = aliases.clone();
            }
            if let Some(ref preamble) = body.preamble {
                pc.worker_preamble = preamble.clone();
            }
            if let Some(ref check_cmd) = body.check_command {
                pc.check_command = check_cmd.clone();
            }
            if let Some(ref summary) = body.scout_summary {
                pc.scout_summary = summary.clone();
            }
            if body.redetect_logo == Some(true) {
                let project_path = std::path::Path::new(&pc.path);
                pc.logo = detect_project_logo(project_path, &pc.name);
            }

            *logo_out.lock().unwrap() = Some(pc.logo.clone());
            Ok(())
        })
        .await
        .map_err(update_err)?;

    let logo = logo.lock().unwrap().clone().flatten();
    Ok(Json(json!({ "ok": true, "logo": logo })))
}

/// DELETE /api/projects/{name} — remove a project and cascade-delete its tasks.
pub(crate) async fn delete_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let key = resolve_project_key(&config, &name)?;

    // Collect all identifiers tasks might use to reference this project.
    let mut identifiers = vec![key.clone()];
    if let Some(pc) = config.captain.projects.get(&key) {
        identifiers.push(pc.name.clone());
        identifiers.extend(pc.aliases.iter().cloned());
        if pc.path != key {
            identifiers.push(pc.path.clone());
        }
    }

    // Cascade-delete tasks belonging to this project (async I/O, before lock).
    let store = state.task_store.read().await;
    let all_tasks = store.load_all_with_archived().await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("task load failed: {e}"),
        )
    })?;
    let task_ids: Vec<i64> = all_tasks
        .iter()
        .filter(|t| {
            t.project
                .as_ref()
                .is_some_and(|p| identifiers.iter().any(|id| id.eq_ignore_ascii_case(p)))
        })
        .map(|t| t.id)
        .collect();
    let deleted_tasks = task_ids.len();

    if !task_ids.is_empty() {
        let opts = mando_captain::io::task_cleanup::CleanupOptions { close_pr: false };
        mando_captain::runtime::dashboard::delete_tasks(&config, &store, &task_ids, &opts)
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("task cleanup failed: {e}"),
                )
            })?;
        state.bus.send(
            mando_types::BusEvent::Tasks,
            Some(json!({"action": "delete"})),
        );
    }

    // Atomically remove the project under write lock.
    state
        .config_manager
        .update(|cfg| {
            cfg.captain.projects.remove(&key);
            Ok(())
        })
        .await
        .map_err(update_err)?;

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
