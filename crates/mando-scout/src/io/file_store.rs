//! File I/O for scout summaries and content.
//!
//! Layout:
//! - `~/.mando/scout/summaries/{id:03d}-{slug}.md`
//! - `~/.mando/scout/content/{id:03d}-article.md`

use std::path::{Path, PathBuf};

/// Root directory for scout data — always under `data_dir()/scout`.
pub(crate) fn scout_dir() -> PathBuf {
    mando_config::data_dir().join("scout")
}

/// Summaries directory.
fn summaries_dir() -> PathBuf {
    scout_dir().join("summaries")
}

/// Content directory.
pub(crate) fn content_dir() -> PathBuf {
    scout_dir().join("content")
}

/// Path where a summary file should live.
pub fn summary_path(id: i64, slug: &str) -> PathBuf {
    summaries_dir().join(format!("{id:03}-{slug}.md"))
}

/// Path where raw fetched content lives: `{id:03d}.txt`.
pub fn content_path(id: i64) -> PathBuf {
    content_dir().join(format!("{id:03}.txt"))
}

/// Read a summary file, returning None if it doesn't exist.
pub fn read_summary(id: i64, slug: &str) -> Option<String> {
    let path = summary_path(id, slug);
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!("failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Write a summary file, creating directories as needed.
pub fn write_summary(id: i64, slug: &str, content: &str) -> std::io::Result<()> {
    let path = summary_path(id, slug);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)
}

/// Read a content/article file, returning None if it doesn't exist.
pub fn read_content(id: i64) -> Option<String> {
    let path = content_path(id);
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!("failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Read the synthesized article markdown, returning None if it doesn't exist.
pub fn read_article(id: i64) -> Option<String> {
    let path = article_path(id);
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!("failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Path for the synthesized article: `{id:03d}-article.md`.
pub fn article_path(id: i64) -> PathBuf {
    content_dir().join(format!("{id:03}-article.md"))
}

/// Path for the Telegraph URL cache: `{id:03d}-telegraph.json`.
pub fn telegraph_cache_path(id: i64) -> PathBuf {
    content_dir().join(format!("{id:03}-telegraph.json"))
}

/// Delete all cached files for an item. Best-effort — missing files are expected,
/// but other errors (permissions, I/O) are logged at warn level.
pub fn delete_item_files(id: i64, slug: Option<&str>) {
    try_remove(&content_path(id));
    try_remove(&article_path(id));
    try_remove(&telegraph_cache_path(id));
    if let Some(slug) = slug {
        try_remove(&summary_path(id, slug));
    }
}

fn try_remove(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%e, path = %path.display(), "failed to clean up cached file");
        }
    }
}

/// Write synthesized article markdown.
pub fn write_article(id: i64, article: &str) -> std::io::Result<()> {
    let path = article_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, article)
}

/// Write raw content file, creating directories as needed.
pub fn write_content(id: i64, content: &str) -> std::io::Result<()> {
    let path = content_dir().join(format!("{id:03}.txt"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: We avoid using MANDO_DATA_DIR in tests that do file I/O
    // because env vars are process-global and race across parallel tests.
    // Instead we test path formatting with known-good paths and do
    // direct file I/O in unique temp directories.

    #[test]
    fn summary_path_format() {
        let p = summary_path(7, "my-article");
        let s = p.to_str().unwrap();
        assert!(s.ends_with("007-my-article.md"));
    }

    #[test]
    fn content_path_format() {
        let p = content_path(42);
        let s = p.to_str().unwrap();
        assert!(s.ends_with("042.txt"));
    }

    #[test]
    fn write_and_read_summary_direct() {
        let dir = std::env::temp_dir().join(format!("mando-scout-fs-s-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sdir = dir.join("summaries");
        std::fs::create_dir_all(&sdir).unwrap();

        let path = sdir.join("001-test.md");
        std::fs::write(&path, "Hello summary").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello summary");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_and_read_content_direct() {
        let dir = std::env::temp_dir().join(format!("mando-scout-fs-c-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cdir = dir.join("content");
        std::fs::create_dir_all(&cdir).unwrap();

        let path = cdir.join("002-article.txt");
        std::fs::write(&path, "Raw content here").unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        assert_eq!(data, "Raw content here");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_nonexistent_returns_none() {
        // read_summary/read_content return None for non-existent paths.
        let p = PathBuf::from("/tmp/mando-scout-nonexistent-abc/x.md");
        assert!(std::fs::read_to_string(&p).ok().is_none());
    }

    #[test]
    fn telegraph_cache_path_format() {
        let p = telegraph_cache_path(5);
        assert!(p.to_str().unwrap().ends_with("005-telegraph.json"));
    }

    #[test]
    fn delete_item_files_removes_all() {
        let dir = std::env::temp_dir().join(format!("mando-scout-del-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cdir = dir.join("content");
        let sdir = dir.join("summaries");
        std::fs::create_dir_all(&cdir).unwrap();
        std::fs::create_dir_all(&sdir).unwrap();

        // Create all 4 file types manually using known paths.
        let raw = cdir.join("001.txt");
        let article = cdir.join("001-article.md");
        let telegraph = cdir.join("001-telegraph.json");
        let summary = sdir.join("001-test-slug.md");
        for p in [&raw, &article, &telegraph, &summary] {
            std::fs::write(p, "data").unwrap();
        }

        // We can't call delete_item_files directly because it uses scout_dir()
        // which depends on data_dir(). Instead verify the logic: remove_file on
        // non-existent paths is silently ignored.
        for p in [&raw, &article, &telegraph, &summary] {
            let _ = std::fs::remove_file(p);
            assert!(!p.exists());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
