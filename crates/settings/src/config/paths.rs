use std::collections::HashMap;

use super::settings::{Config, ProjectConfig};

pub fn match_project_by_prefix<'a>(
    text: &'a str,
    projects: &HashMap<String, ProjectConfig>,
) -> (Option<String>, &'a str) {
    let trimmed = text.trim();
    let boundary = trimmed
        .find(|c: char| c == ':' || c.is_whitespace())
        .unwrap_or(trimmed.len());
    if boundary == 0 || boundary == trimmed.len() {
        return (None, text);
    }

    let candidate = &trimmed[..boundary];
    let candidate_lower = candidate.to_lowercase();

    let rest = trimmed[boundary..].trim_start_matches(':').trim_start();
    if rest.is_empty() {
        return (None, text);
    }

    for pc in projects.values() {
        let name_lower = pc.name.to_lowercase();
        if candidate_lower == name_lower {
            return (Some(pc.name.clone()), rest);
        }
        for alias in &pc.aliases {
            if candidate_lower == alias.to_lowercase() {
                return (Some(pc.name.clone()), rest);
            }
        }
    }

    (None, text)
}

pub fn resolve_project_config<'a>(
    project_name: Option<&str>,
    cfg: &'a Config,
) -> Option<(&'a str, &'a ProjectConfig)> {
    let name = project_name?;
    let projects = &cfg.captain.projects;
    let name_lower = name.to_lowercase();
    for (key, pc) in projects {
        if pc.name.to_lowercase() == name_lower {
            return Some((key.as_str(), pc));
        }
    }
    for (key, pc) in projects {
        if pc.aliases.iter().any(|a| a.to_lowercase() == name_lower) {
            return Some((key.as_str(), pc));
        }
    }
    if let Some((k, pc)) = projects.get_key_value(name) {
        return Some((k.as_str(), pc));
    }
    None
}

pub fn resolve_github_repo(project: Option<&str>, config: &Config) -> Option<String> {
    let name = project?;
    let (_, pc) = resolve_project_config(Some(name), config)?;
    pc.github_repo.clone()
}

pub fn first_project_path(cfg: &Config) -> Option<String> {
    cfg.captain
        .projects
        .values()
        .next()
        .map(|rc| rc.path.clone())
}

pub fn detect_github_repo(path: &str) -> Option<String> {
    let abs = global_infra::paths::expand_tilde(path);
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

pub fn parse_github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let slug = rest.trim_end_matches(".git");
        if slug.contains('/') {
            return Some(slug.to_string());
        }
    }
    if let Some(idx) = url.find("github.com/") {
        let slug = url[idx + "github.com/".len()..].trim_end_matches(".git");
        if slug.contains('/') && !slug.contains(' ') {
            return Some(slug.to_string());
        }
    }
    None
}
