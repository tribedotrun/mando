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
    // save_config is synchronous — move to the blocking pool so we don't
    // stall the async executor while holding config_write_mu.
    let to_save = config.clone();
    match tokio::task::spawn_blocking(move || mando_config::save_config(&to_save, None)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("save failed: {e}"),
            ));
        }
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("save task panicked: {e}"),
            ));
        }
    }

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

    // Auto-generate scout summary from project metadata.
    let scout_summary = detect_project_summary(&abs_path).await;

    // Auto-detect project logo.
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

    config.captain.projects.insert(key, pc);
    save_and_reload(&state, &config).await?;

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
    if let Some(summary) = body.scout_summary {
        pc.scout_summary = summary;
    }
    if body.redetect_logo == Some(true) {
        let project_path = std::path::Path::new(&pc.path);
        pc.logo = detect_project_logo(project_path, &pc.name);
    }

    let logo = pc.logo.clone();
    save_and_reload(&state, &config).await?;
    Ok(Json(json!({ "ok": true, "logo": logo })))
}

/// DELETE /api/projects/{name} — remove a project and cascade-delete its tasks.
pub(crate) async fn delete_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _write_guard = state.config_write_mu.lock().await;
    let mut config = (*state.config.load_full()).clone();
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

    // Cascade-delete tasks belonging to this project.
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

    config.captain.projects.remove(&key);
    save_and_reload(&state, &config).await?;

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

/// Candidate paths for project logo files, checked in priority order.
const LOGO_CANDIDATES: &[&str] = &[
    "logo.png",
    "logo.svg",
    "logo.jpg",
    "logo.webp",
    "public/logo.png",
    "public/logo.svg",
    "public/favicon.ico",
    "public/favicon.png",
    "public/favicon.svg",
    "assets/logo.png",
    "assets/icon.png",
    "src/assets/logo.png",
    "src/assets/icon.png",
    ".github/logo.png",
    ".github/icon.png",
    "electron/assets/icon.png",
    "icon.png",
    "icon.svg",
    "icon.ico",
    "favicon.ico",
];

/// Auto-detect a logo image from the project directory, copy it to
/// `~/.mando/images/`, and return the stored filename.
fn detect_project_logo(project_path: &std::path::Path, project_name: &str) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let source = LOGO_CANDIDATES
        .iter()
        .map(|c| project_path.join(c))
        .find(|p| p.is_file())?;

    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png");

    // Sanitize project name for filename.
    let safe_name: String = project_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase();

    // Include a short path hash to avoid collisions between projects whose
    // sanitized names are identical (e.g. "my_app" and "my-app").
    let mut hasher = DefaultHasher::new();
    project_path.hash(&mut hasher);
    let path_hash = format!("{:08x}", hasher.finish() & 0xFFFF_FFFF);

    let filename = format!("project-{safe_name}-{path_hash}.{ext}");
    let dest_dir = mando_config::images_dir();
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        tracing::warn!(
            project = project_name,
            dir = %dest_dir.display(),
            error = %e,
            "failed to create images directory for project logo"
        );
        return None;
    }
    let dest = dest_dir.join(&filename);

    match std::fs::copy(&source, &dest) {
        Ok(_) => Some(filename),
        Err(e) => {
            tracing::warn!(
                project = project_name,
                source = %source.display(),
                error = %e,
                "failed to copy project logo"
            );
            None
        }
    }
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
