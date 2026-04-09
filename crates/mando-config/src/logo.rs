//! Project logo auto-detection — find a logo image in a project directory,
//! copy it to `~/.mando/images/`, and return the stored filename.
//!
//! Checks well-known static paths covering common project layouts:
//! single-app repos, monorepos, Electron, Next.js, Expo/RN, Docusaurus,
//! Sphinx, and generic conventions.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::paths::images_dir;

/// Candidate paths for project logo files, checked in priority order.
/// Only includes paths where the file is almost certainly the project's
/// branding — not arbitrary icons buried in the tree.
pub const LOGO_CANDIDATES: &[&str] = &[
    // ── Root ────────────────────────────────────────────────────────
    "logo.png",
    "logo.svg",
    "logo.jpg",
    "logo.webp",
    // ── public/ (CRA, Vite, Nuxt, SvelteKit, etc.) ─────────────────
    "public/logo.png",
    "public/logo.svg",
    "public/favicon.ico",
    "public/favicon.png",
    "public/favicon.svg",
    // ── assets/ ─────────────────────────────────────────────────────
    "assets/logo.png",
    "assets/logo.svg",
    "assets/icon.png",
    "assets/icon.svg",
    "assets/images/logo.png",
    "assets/images/logo.svg",
    "assets/images/icon.png",
    // ── src/assets/ ─────────────────────────────────────────────────
    "src/assets/logo.png",
    "src/assets/logo.svg",
    "src/assets/icon.png",
    "src/assets/icon.svg",
    "src/assets/images/logo.png",
    "src/assets/images/logo.svg",
    // ── Next.js app router (icon.* = favicon convention) ────────────
    "app/icon.png",
    "app/icon.svg",
    "app/favicon.ico",
    "src/app/icon.png",
    "src/app/icon.svg",
    "src/app/favicon.ico",
    // ── Electron ────────────────────────────────────────────────────
    "electron/assets/icon.png",
    "electron/resources/icon.png",
    // ── Expo / React Native monorepo common layouts ─────────────────
    "apps/mobile/assets/icon.png",
    "apps/mobile/assets/logo.png",
    "apps/mobile/assets/images/icon.png",
    "apps/mobile/assets/images/logo.png",
    "apps/app/assets/icon.png",
    "apps/app/assets/logo.png",
    "apps/app/assets/images/icon.png",
    "apps/app/assets/images/logo.png",
    // ── Web monorepo common layouts ─────────────────────────────────
    "apps/web/public/logo.png",
    "apps/web/public/logo.svg",
    "apps/web/public/favicon.ico",
    "apps/web/app/icon.png",
    "apps/web/app/icon.svg",
    "packages/app/assets/logo.png",
    "packages/app/assets/icon.png",
    // ── Docusaurus / static sites ───────────────────────────────────
    "static/img/logo.svg",
    "static/img/logo.png",
    // ── Sphinx / documentation ──────────────────────────────────────
    "docs/logo.png",
    "docs/logo.svg",
    "docs/_static/logo.png",
    "docs/_static/logo.svg",
    // ── .github/ ────────────────────────────────────────────────────
    ".github/logo.png",
    ".github/icon.png",
    // ── Generic directories ─────────────────────────────────────────
    "resources/logo.png",
    "resources/icon.png",
    "images/logo.png",
    "images/logo.svg",
    "img/logo.png",
    "img/logo.svg",
    // ── Root icon/favicon ───────────────────────────────────────────
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

    copy_logo_to_images(&source, project_path, project_name)
}

/// Copy a detected logo file to `~/.mando/images/` and return the stored filename.
fn copy_logo_to_images(source: &Path, project_path: &Path, project_name: &str) -> Option<String> {
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

    match std::fs::copy(source, &dest) {
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
