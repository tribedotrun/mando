//! Project logo auto-detection — find a logo image in a project directory,
//! copy it to `~/.mando/images/`, and return the stored filename.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::paths::images_dir;

/// Candidate paths for project logo files, checked in priority order.
pub const LOGO_CANDIDATES: &[&str] = &[
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

/// Detect a logo image from the project directory, copy it to
/// `~/.mando/images/`, and return the stored filename.
pub fn detect_project_logo(project_path: &Path, project_name: &str) -> Option<String> {
    let source = LOGO_CANDIDATES
        .iter()
        .map(|c| project_path.join(c))
        .find(|p| p.is_file())?;

    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png");

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

    let mut hasher = DefaultHasher::new();
    project_path.hash(&mut hasher);
    let path_hash = format!("{:08x}", hasher.finish() & 0xFFFF_FFFF);

    let filename = format!("project-{safe_name}-{path_hash}.{ext}");
    let dest_dir = images_dir();
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

/// Scan all projects and detect logos for any that don't have one.
/// Returns `true` if any project was updated (caller should save config).
pub fn backfill_project_logos(config: &mut crate::Config) -> bool {
    let mut changed = false;
    for pc in config.captain.projects.values_mut() {
        if pc.logo.is_some() {
            continue;
        }
        let project_path = Path::new(&pc.path);
        if !project_path.is_dir() {
            continue;
        }
        if let Some(logo) = detect_project_logo(project_path, &pc.name) {
            tracing::info!(project = pc.name, logo = logo, "backfilled project logo");
            pc.logo = Some(logo);
            changed = true;
        }
    }
    changed
}
