//! /api/worktrees/* route handlers.

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;

use mando_captain::io::git;

use crate::response::{error_response, internal_error};
use crate::AppState;

/// Resolve a project from config by name or alias (case-insensitive).
fn resolve_project<'a>(
    config: &'a mando_config::Config,
    name: Option<&str>,
) -> Result<(&'a str, &'a mando_config::settings::ProjectConfig), (StatusCode, Json<Value>)> {
    let projects = &config.captain.projects;
    match name {
        Some(name) => {
            let name_lower = name.to_lowercase();
            for (k, v) in projects {
                if v.name.to_lowercase() == name_lower
                    || v.aliases.iter().any(|a| a.to_lowercase() == name_lower)
                {
                    return Ok((k.as_str(), v));
                }
            }
            Err(error_response(
                StatusCode::NOT_FOUND,
                &format!("project not found: {name}"),
            ))
        }
        None => Err(error_response(
            StatusCode::BAD_REQUEST,
            "project name required",
        )),
    }
}

/// Find the project that owns a worktree path by checking the central worktrees dir.
/// Uses longest-prefix-wins to handle overlapping repo names (e.g. `foo` vs `foo-bar`).
fn find_project_for_worktree(config: &mando_config::Config, wt_path: &Path) -> Option<PathBuf> {
    let wt_dir = git::worktrees_dir();
    if !wt_path.starts_with(&wt_dir) {
        return None;
    }
    let wt_name = wt_path.file_name().and_then(|n| n.to_str())?;
    let mut best: Option<(usize, PathBuf)> = None;
    for pc in config.captain.projects.values() {
        let project_path = mando_config::expand_tilde(&pc.path);
        let prefix = format!("{}-", repo_dir_name(&project_path));
        if wt_name.starts_with(&prefix) && best.as_ref().is_none_or(|(len, _)| prefix.len() > *len)
        {
            best = Some((prefix.len(), project_path));
        }
    }
    best.map(|(_, path)| path)
}

/// Get the repo directory name (used as prefix filter for orphan detection).
fn repo_dir_name(project_path: &Path) -> String {
    project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

#[derive(Deserialize)]
pub(crate) struct CreateWorktreeBody {
    pub name: Option<String>,
    pub project: Option<String>,
}

/// POST /api/worktrees — create a worktree.
pub(crate) async fn post_worktrees(
    State(state): State<AppState>,
    Json(body): Json<CreateWorktreeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let (project_key, project_cfg) = resolve_project(&config, body.project.as_deref())?;
    let project_path = mando_config::expand_tilde(&project_cfg.path);

    let suffix = match &body.name {
        Some(n) => n.clone(),
        None => {
            let now = OffsetDateTime::now_utc();
            format!(
                "{:02}{:02}-{:02}{:02}",
                now.month() as u8,
                now.day(),
                now.hour(),
                now.minute()
            )
        }
    };

    let branch = format!("worktree-{suffix}");
    let wt_path = git::worktree_path(&project_path, &suffix);

    git::fetch_origin(&project_path)
        .await
        .map_err(internal_error)?;

    let default_br = git::default_branch(&project_path)
        .await
        .map_err(internal_error)?;

    // Clean up stale worktree/branch if they exist.
    if wt_path.exists() {
        if let Err(e) = git::remove_worktree(&project_path, &wt_path).await {
            tracing::warn!(
                module = "worktrees",
                path = %wt_path.display(),
                error = %e,
                "failed to remove stale worktree"
            );
            if wt_path.exists() {
                return Err(error_response(
                    StatusCode::CONFLICT,
                    &format!(
                        "worktree exists at {} and could not be removed: {e}",
                        wt_path.display()
                    ),
                ));
            }
        }
    }
    if let Err(e) = git::delete_local_branch(&project_path, &branch).await {
        tracing::debug!(
            module = "worktrees",
            branch = %branch,
            error = %e,
            "stale branch cleanup (expected if branch doesn't exist)"
        );
    }

    git::create_worktree(&project_path, &branch, &wt_path, &default_br)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({
        "ok": true,
        "path": wt_path.to_string_lossy(),
        "branch": branch,
        "project": project_key,
    })))
}

/// GET /api/worktrees — list all worktrees across projects.
pub(crate) async fn get_worktrees(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let mut worktrees = Vec::new();

    for (project_name, pc) in &config.captain.projects {
        let project_path = mando_config::expand_tilde(&pc.path);
        match git::list_worktrees(&project_path).await {
            Ok(paths) => {
                for p in paths {
                    worktrees.push(json!({
                        "project": project_name,
                        "path": p.to_string_lossy(),
                    }));
                }
            }
            Err(e) => {
                tracing::warn!(
                    module = "worktrees",
                    project = project_name.as_str(),
                    error = %e,
                    "failed to list worktrees"
                );
            }
        }
    }

    Ok(Json(json!({ "worktrees": worktrees })))
}

/// POST /api/worktrees/prune — prune stale worktrees for all projects.
pub(crate) async fn post_worktrees_prune(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let mut pruned = Vec::new();

    for (project_name, pc) in &config.captain.projects {
        let project_path = mando_config::expand_tilde(&pc.path);
        let output = tokio::process::Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&project_path)
            .output()
            .await;

        match output {
            Ok(o) if o.status.success() => {
                pruned.push(project_name.clone());
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!(
                    module = "worktrees",
                    project = project_name.as_str(),
                    stderr = %stderr.trim(),
                    "git worktree prune failed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    module = "worktrees",
                    project = project_name.as_str(),
                    error = %e,
                    "git worktree prune failed"
                );
            }
        }
    }

    Ok(Json(json!({ "ok": true, "pruned": pruned })))
}

#[derive(Deserialize)]
pub(crate) struct RemoveWorktreeBody {
    pub path: String,
}

/// POST /api/worktrees/remove — remove a specific worktree by full path.
pub(crate) async fn post_worktrees_remove(
    State(state): State<AppState>,
    Json(body): Json<RemoveWorktreeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let wt_path = PathBuf::from(&body.path);

    let repo_path = find_project_for_worktree(&config, &wt_path).ok_or_else(|| {
        error_response(StatusCode::NOT_FOUND, "no project owns this worktree path")
    })?;

    git::remove_worktree(&repo_path, &wt_path)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(crate) struct CleanupBody {
    #[serde(default)]
    pub dry_run: bool,
}

/// POST /api/worktrees/cleanup — find and optionally remove orphan worktree dirs.
pub(crate) async fn post_worktrees_cleanup(
    State(state): State<AppState>,
    Json(body): Json<CleanupBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let mut orphans = Vec::new();
    let mut removed = Vec::new();

    // Collect tracked worktrees from ALL projects first to avoid cross-project deletions.
    let mut all_tracked = std::collections::HashSet::new();
    let mut project_prefixes = Vec::new();
    for pc in config.captain.projects.values() {
        let project_path = mando_config::expand_tilde(&pc.path);
        let prefix = format!("{}-", repo_dir_name(&project_path));

        if !body.dry_run {
            let _ = tokio::process::Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&project_path)
                .output()
                .await;
        }

        match git::list_worktrees(&project_path).await {
            Ok(paths) => {
                all_tracked.extend(paths);
                project_prefixes.push(prefix);
            }
            Err(e) => {
                tracing::warn!(
                    module = "worktrees",
                    project = %pc.name,
                    error = %e,
                    "failed to list worktrees, skipping project in orphan scan"
                );
            }
        }
    }

    // Scan central worktrees directory once.
    let wt_dir = git::worktrees_dir();
    let mut entries = match tokio::fs::read_dir(&wt_dir).await {
        Ok(e) => e,
        Err(_) => {
            return Ok(Json(
                json!({"ok": true, "orphans": orphans, "removed": removed}),
            ))
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        // Only consider dirs matching a known project prefix (longest wins).
        let owned = project_prefixes
            .iter()
            .filter(|pfx| dir_name.starts_with(pfx.as_str()))
            .max_by_key(|pfx| pfx.len())
            .is_some();
        if !owned || all_tracked.contains(&path) {
            continue;
        }

        let path_str = path.to_string_lossy().into_owned();
        orphans.push(path_str.clone());

        if !body.dry_run {
            if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                tracing::warn!(
                    module = "worktrees",
                    path = %path.display(),
                    error = %e,
                    "failed to remove orphan worktree dir"
                );
            } else {
                removed.push(path_str);
            }
        }
    }

    Ok(Json(json!({
        "ok": true,
        "orphans": orphans,
        "removed": removed,
    })))
}
