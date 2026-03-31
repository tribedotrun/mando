//! Path constants and helpers for ~/.mando/ layout.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::settings::{Config, ProjectConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptainRuntimePaths {
    pub task_db_path: PathBuf,
    pub lockfile_path: PathBuf,
    pub worker_health_path: PathBuf,
}

fn default_captain_runtime_paths() -> CaptainRuntimePaths {
    CaptainRuntimePaths {
        task_db_path: data_dir().join("mando.db"),
        lockfile_path: data_dir().join("captain.lock"),
        worker_health_path: state_dir().join("worker-health.json"),
    }
}

fn captain_runtime_paths_cell() -> &'static RwLock<Option<CaptainRuntimePaths>> {
    static CELL: OnceLock<RwLock<Option<CaptainRuntimePaths>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(None))
}

fn read_runtime_paths() -> Option<CaptainRuntimePaths> {
    captain_runtime_paths_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn resolve_captain_runtime_paths(config: &Config) -> CaptainRuntimePaths {
    CaptainRuntimePaths {
        task_db_path: expand_tilde(&config.captain.task_db_path),
        lockfile_path: expand_tilde(&config.captain.lockfile_path),
        worker_health_path: expand_tilde(&config.captain.worker_health_path),
    }
}

pub fn set_active_captain_runtime_paths(paths: CaptainRuntimePaths) {
    *captain_runtime_paths_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(paths);
}

pub fn active_captain_runtime_paths() -> CaptainRuntimePaths {
    read_runtime_paths().unwrap_or_else(default_captain_runtime_paths)
}

pub fn captain_paths_restart_required(config: &Config) -> bool {
    active_captain_runtime_paths() != resolve_captain_runtime_paths(config)
}

#[cfg(test)]
pub fn clear_active_captain_runtime_paths_for_test() {
    *captain_runtime_paths_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// Return the Mando data directory (`~/.mando` or `MANDO_DATA_DIR`).
pub fn data_dir() -> PathBuf {
    mando_types::data_dir()
}

/// `~/.mando/state`
pub fn state_dir() -> PathBuf {
    data_dir().join("state")
}

/// `~/.mando/logs`
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// `~/.mando/images`
pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}

/// `~/.mando/bin` — managed external tool binaries.
pub fn bin_dir() -> PathBuf {
    data_dir().join("bin")
}

/// `~/.mando/mando.db` — unified SQLite database.
pub fn task_db_path() -> PathBuf {
    active_captain_runtime_paths().task_db_path
}

/// `~/.mando/captain.lock`
pub fn captain_lock_path() -> PathBuf {
    active_captain_runtime_paths().lockfile_path
}

/// `~/.mando/state/worker-health.json`
pub fn worker_health_path() -> PathBuf {
    active_captain_runtime_paths().worker_health_path
}

/// `~/.mando/state/cron/jobs.json`
pub fn cron_store_path() -> PathBuf {
    state_dir().join("cron").join("jobs.json")
}

/// `~/.mando/state/cc-streams/` — unified stream output for all CC invocations.
pub fn cc_streams_dir() -> PathBuf {
    state_dir().join("cc-streams")
}

/// Stream JSONL path for a given session ID.
pub fn stream_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.jsonl"))
}

/// Stream metadata path for a given session ID.
pub fn stream_meta_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.meta.json"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(p: &str) -> PathBuf {
    mando_types::expand_tilde(p)
}

/// Convert a title to a URL-safe slug: lowercase, non-alnum replaced with
/// hyphens, collapsed, trimmed, and truncated to `max_len`.
pub fn slugify(title: &str, max_len: usize) -> String {
    let mut slug = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push('-');
        }
    }
    // Collapse consecutive hyphens.
    let collapsed = collapse_hyphens(&slug);
    // Trim leading/trailing hyphens.
    let trimmed = collapsed.trim_matches('-');
    // Truncate to max_len, but don't break in the middle of a word if avoidable.
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    let truncated = &trimmed[..max_len];
    // Trim trailing hyphen from truncation.
    truncated.trim_end_matches('-').to_string()
}

fn collapse_hyphens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_hyphen = false;
    for ch in s.chars() {
        if ch == '-' {
            if !prev_hyphen {
                out.push('-');
            }
            prev_hyphen = true;
        } else {
            out.push(ch);
            prev_hyphen = false;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Project utils
// ---------------------------------------------------------------------------

/// Check if `text` starts with a project name or alias prefix.
///
/// Returns `(matched_name, cleaned_title)` or `(None, original_text)`.
pub fn match_project_by_prefix<'a>(
    text: &'a str,
    projects: &HashMap<String, ProjectConfig>,
) -> (Option<String>, &'a str) {
    let trimmed = text.trim();
    // Find the first whitespace or colon+whitespace boundary.
    let boundary = trimmed
        .find(|c: char| c == ':' || c.is_whitespace())
        .unwrap_or(trimmed.len());
    if boundary == 0 || boundary == trimmed.len() {
        return (None, text);
    }

    let candidate = &trimmed[..boundary];
    let candidate_lower = candidate.to_lowercase();

    // Skip past the colon/whitespace to get the cleaned remainder.
    let rest = trimmed[boundary..].trim_start_matches(':').trim_start();
    if rest.is_empty() {
        return (None, text);
    }

    for pc in projects.values() {
        // Match against project name.
        let name_lower = pc.name.to_lowercase();
        if candidate_lower == name_lower {
            return (Some(pc.name.clone()), rest);
        }
        // Match against explicit aliases.
        for alias in &pc.aliases {
            if candidate_lower == alias.to_lowercase() {
                return (Some(pc.name.clone()), rest);
            }
        }
    }

    (None, text)
}

/// Resolve a project config by identifier.
///
/// Resolution steps: name → alias → direct path key.
/// Returns `None` when `project_name` is `None` or no match is found.
pub fn resolve_project_config<'a>(
    project_name: Option<&str>,
    cfg: &'a Config,
) -> Option<(&'a str, &'a ProjectConfig)> {
    let name = project_name?;
    let projects = &cfg.captain.projects;
    let name_lower = name.to_lowercase();
    // Match by name.
    for (key, pc) in projects {
        if pc.name.to_lowercase() == name_lower {
            return Some((key.as_str(), pc));
        }
    }
    // Match by alias.
    for (key, pc) in projects {
        if pc.aliases.iter().any(|a| a.to_lowercase() == name_lower) {
            return Some((key.as_str(), pc));
        }
    }
    // Direct path key match (the HashMap key is the absolute path).
    if let Some((k, pc)) = projects.get_key_value(name) {
        return Some((k.as_str(), pc));
    }
    None
}

/// Resolve a project display-name to its `github_repo` slug from config.
pub fn resolve_github_repo(project: Option<&str>, config: &Config) -> Option<String> {
    let name = project?;
    let (_, pc) = resolve_project_config(Some(name), config)?;
    pc.github_repo.clone()
}

/// Return the path of the first configured project (for global operations).
pub fn first_project_path(cfg: &Config) -> Option<String> {
    cfg.captain
        .projects
        .values()
        .next()
        .map(|rc| rc.path.clone())
}

// ---------------------------------------------------------------------------
// GitHub repo detection
// ---------------------------------------------------------------------------

/// Detect the GitHub repo slug (owner/repo) from `git remote get-url origin`
/// at the given path. Returns `None` if the path is not a git repo or the
/// remote is not a GitHub URL.
pub fn detect_github_repo(path: &str) -> Option<String> {
    let abs = expand_tilde(path);
    let child = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&abs)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_slug(&url)
}

/// Parse a GitHub slug from a git remote URL.
///
/// Handles SSH (`git@github.com:owner/repo.git`),
/// HTTPS (`https://github.com/owner/repo.git`), and bare forms.
pub fn parse_github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let slug = rest.trim_end_matches(".git");
        if slug.contains('/') {
            return Some(slug.to_string());
        }
    }
    // HTTPS: https://github.com/owner/repo.git
    if url.contains("github.com/") {
        if let Some(idx) = url.find("github.com/") {
            let slug = url[idx + "github.com/".len()..].trim_end_matches(".git");
            if slug.contains('/') && !slug.contains(' ') {
                return Some(slug.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Convenience: project resolution for CLI callers.
// ---------------------------------------------------------------------------
