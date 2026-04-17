//! Project logo auto-detection -- candidate paths and filename generation.
//!
//! Filesystem I/O lives in `crate::io::logo`.

/// Candidate paths for project logo files, checked in priority order.
pub const LOGO_CANDIDATES: &[&str] = &[
    // -- Root
    "logo.png",
    "logo.svg",
    "logo.jpg",
    "logo.webp",
    // -- public/ (CRA, Vite, Nuxt, SvelteKit, etc.)
    "public/logo.png",
    "public/logo.svg",
    "public/favicon.ico",
    "public/favicon.png",
    "public/favicon.svg",
    // -- assets/
    "assets/logo.png",
    "assets/logo.svg",
    "assets/icon.png",
    "assets/icon.svg",
    "assets/images/logo.png",
    "assets/images/logo.svg",
    "assets/images/icon.png",
    // -- src/assets/
    "src/assets/logo.png",
    "src/assets/logo.svg",
    "src/assets/icon.png",
    "src/assets/icon.svg",
    "src/assets/images/logo.png",
    "src/assets/images/logo.svg",
    // -- Next.js app router (icon.* = favicon convention)
    "app/icon.png",
    "app/icon.svg",
    "app/favicon.ico",
    "src/app/icon.png",
    "src/app/icon.svg",
    "src/app/favicon.ico",
    // -- Electron
    "electron/assets/icon.png",
    "electron/resources/icon.png",
    // -- Expo / React Native monorepo common layouts
    "apps/mobile/assets/icon.png",
    "apps/mobile/assets/logo.png",
    "apps/mobile/assets/images/icon.png",
    "apps/mobile/assets/images/logo.png",
    "apps/app/assets/icon.png",
    "apps/app/assets/logo.png",
    "apps/app/assets/images/icon.png",
    "apps/app/assets/images/logo.png",
    // -- Web monorepo common layouts
    "apps/web/public/logo.png",
    "apps/web/public/logo.svg",
    "apps/web/public/favicon.ico",
    "apps/web/app/icon.png",
    "apps/web/app/icon.svg",
    "packages/app/assets/logo.png",
    "packages/app/assets/icon.png",
    // -- Docusaurus / static sites
    "static/img/logo.svg",
    "static/img/logo.png",
    // -- Sphinx / documentation
    "docs/logo.png",
    "docs/logo.svg",
    "docs/_static/logo.png",
    "docs/_static/logo.svg",
    // -- .github/
    ".github/logo.png",
    ".github/icon.png",
    // -- Generic directories
    "resources/logo.png",
    "resources/icon.png",
    "images/logo.png",
    "images/logo.svg",
    "img/logo.png",
    "img/logo.svg",
    // -- Root icon/favicon
    "icon.png",
    "icon.svg",
    "icon.ico",
    "favicon.ico",
];

/// Generate a stable destination filename for a project logo.
pub fn logo_filename(project_name: &str, project_path: &std::path::Path, ext: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

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

    format!("project-{safe_name}-{path_hash}.{ext}")
}
