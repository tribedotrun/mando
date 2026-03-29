//! Tests for path utilities: project resolution and loader.

use std::collections::HashMap;

use crate::paths::{match_project_by_prefix, resolve_project_config};
use crate::settings::{Config, ProjectConfig};

// ---------------------------------------------------------------------------
// paths: match_project_by_prefix
// ---------------------------------------------------------------------------

#[test]
fn match_project_prefix_alias() {
    let mut projects = HashMap::new();
    projects.insert(
        "/code/tc".to_string(),
        ProjectConfig {
            name: "mando".into(),
            path: "/code/tc".into(),
            github_repo: Some("acme/widgets".into()),
            aliases: vec!["tc".into(), "mdo".into()],
            ..Default::default()
        },
    );
    projects.insert(
        "/code/ht".to_string(),
        ProjectConfig {
            name: "acme-web".into(),
            path: "/code/ht".into(),
            github_repo: Some("test-org/acme-web".into()),
            aliases: vec!["ht".into()],
            ..Default::default()
        },
    );

    // Alias match with colon.
    let (name, cleaned) = match_project_by_prefix("tc: fix bug", &projects);
    assert_eq!(name.unwrap(), "mando");
    assert_eq!(cleaned, "fix bug");

    // Alias match without colon.
    let (name, cleaned) = match_project_by_prefix("ht add feature", &projects);
    assert_eq!(name.unwrap(), "acme-web");
    assert_eq!(cleaned, "add feature");

    // Name match (case-insensitive).
    let (name, cleaned) = match_project_by_prefix("mando do stuff", &projects);
    assert_eq!(name.unwrap(), "mando");
    assert_eq!(cleaned, "do stuff");

    // No match.
    let (name, text) = match_project_by_prefix("unknown: blah", &projects);
    assert!(name.is_none());
    assert_eq!(text, "unknown: blah");
}

// ---------------------------------------------------------------------------
// paths: resolve_project_config
// ---------------------------------------------------------------------------

#[test]
fn resolve_project_config_by_name() {
    let mut cfg = Config::default();
    cfg.captain.projects.insert(
        "/code".into(),
        ProjectConfig {
            name: "repo".into(),
            path: "/code".into(),
            github_repo: Some("org/repo".into()),
            aliases: vec!["rp".into()],
            ..Default::default()
        },
    );

    // Explicit name match.
    let result = resolve_project_config(Some("repo"), &cfg);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1.name, "repo");

    // Alias match.
    let result = resolve_project_config(Some("rp"), &cfg);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1.name, "repo");

    // Direct key match (absolute path).
    let result = resolve_project_config(Some("/code"), &cfg);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1.name, "repo");

    // None returns None (no implicit fallback).
    let result = resolve_project_config(None, &cfg);
    assert!(result.is_none());

    // Unknown name returns None.
    let result = resolve_project_config(Some("unknown"), &cfg);
    assert!(result.is_none());

    // github_repo slug does NOT resolve — canonical model uses project names only.
    let result = resolve_project_config(Some("org/repo"), &cfg);
    assert!(result.is_none());
}

#[test]
fn resolve_project_config_multiple_projects() {
    let mut cfg = Config::default();
    cfg.captain.projects.insert(
        "/code/alpha".into(),
        ProjectConfig {
            name: "alpha".into(),
            path: "/code/alpha".into(),
            ..Default::default()
        },
    );
    cfg.captain.projects.insert(
        "/code/beta".into(),
        ProjectConfig {
            name: "beta".into(),
            path: "/code/beta".into(),
            ..Default::default()
        },
    );

    // Resolves by name.
    let result = resolve_project_config(Some("alpha"), &cfg);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1.path, "/code/alpha");

    // None returns None with multiple projects (no implicit selection).
    let result = resolve_project_config(None, &cfg);
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// paths: parse_github_slug
// ---------------------------------------------------------------------------

#[test]
fn parse_github_slug_formats() {
    use crate::paths::parse_github_slug;
    assert_eq!(
        parse_github_slug("git@github.com:acme/widgets.git"),
        Some("acme/widgets".into())
    );
    assert_eq!(
        parse_github_slug("https://github.com/acme/widgets.git"),
        Some("acme/widgets".into())
    );
    assert_eq!(
        parse_github_slug("https://github.com/acme/widgets"),
        Some("acme/widgets".into())
    );
    // Non-GitHub
    assert_eq!(parse_github_slug("git@gitlab.com:org/repo.git"), None);
}

// ---------------------------------------------------------------------------
// loader: missing config returns default
// ---------------------------------------------------------------------------

#[test]
fn load_missing_config_returns_default() {
    use std::path::Path;
    let cfg = crate::loader::load_config(Some(Path::new("/nonexistent/config.json")));
    assert_eq!(cfg.workspace, "~/.mando/workspace");
    assert_eq!(cfg.gateway.port, 18790);
}

// ---------------------------------------------------------------------------
// loader: load + save roundtrip via temp file
// ---------------------------------------------------------------------------

#[test]
fn load_save_roundtrip() {
    let dir = std::env::temp_dir().join("mando-config-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test-config.json");

    let mut cfg = Config {
        workspace: "~/test-workspace".into(),
        ..Config::default()
    };
    cfg.gateway.port = 9999;
    cfg.captain.auto_schedule = true;

    crate::loader::save_config(&cfg, Some(&path)).unwrap();
    assert!(path.exists());

    // Load it back (without env overlay, using explicit path).
    let content = std::fs::read_to_string(&path).unwrap();
    let loaded: Config = serde_json::from_str(&content).unwrap();
    assert_eq!(loaded.workspace, "~/test-workspace");
    assert_eq!(loaded.gateway.port, 9999);
    assert!(loaded.captain.auto_schedule);

    // Cleanup.
    let _ = std::fs::remove_dir_all(&dir);
}
