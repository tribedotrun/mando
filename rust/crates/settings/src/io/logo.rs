//! Logo filesystem I/O -- detect, copy, and backfill project logos.

use std::path::Path;

use global_infra::paths::images_dir;

use crate::config::logo::{logo_filename, LOGO_CANDIDATES};

/// Detect a logo image from the project directory, copy it to
/// `~/.mando/images/`, and return the stored filename.
pub fn detect_project_logo(project_path: &Path, project_name: &str) -> Option<String> {
    let source = LOGO_CANDIDATES
        .iter()
        .map(|c| project_path.join(c))
        .find(|p| p.is_file())?;

    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let filename = logo_filename(project_name, project_path, ext);
    let dest_dir = images_dir();

    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        tracing::warn!(
            module = "settings-io-logo", project = project_name,
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
                module = "settings-io-logo", project = project_name,
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
pub fn backfill_project_logos(config: &mut crate::config::Config) -> bool {
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
            tracing::info!(
                module = "settings-io-logo",
                project = pc.name,
                logo = logo,
                "backfilled project logo"
            );
            pc.logo = Some(logo);
            changed = true;
        }
    }
    changed
}
