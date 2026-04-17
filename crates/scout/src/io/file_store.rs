//! File I/O for scout raw content and Telegraph cache.
//!
//! Summaries and articles live in the database (scout_items.summary /
//! scout_items.article). Only the large raw-content transcript and the
//! per-item Telegraph URL cache remain on disk.
//!
//! Layout:
//! - `~/.mando/scout/content/{id:03d}.txt`          — raw fetched content
//! - `~/.mando/scout/content/{id:03d}-telegraph.json` — Telegraph URL cache

use std::path::{Path, PathBuf};

/// Root directory for scout data — always under `data_dir()/scout`.
pub(crate) fn scout_dir() -> PathBuf {
    global_infra::paths::data_dir().join("scout")
}

/// Content directory.
pub(crate) fn content_dir() -> PathBuf {
    scout_dir().join("content")
}

/// Path where raw fetched content lives: `{id:03d}.txt`.
pub fn content_path(id: i64) -> PathBuf {
    content_dir().join(format!("{id:03}.txt"))
}

/// Path for the Telegraph URL cache: `{id:03d}-telegraph.json`.
pub fn telegraph_cache_path(id: i64) -> PathBuf {
    content_dir().join(format!("{id:03}-telegraph.json"))
}

/// Read a file, returning None if missing. Logs a warning on other errors.
fn read_optional(path: &std::path::Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!("failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Async variant of [`read_optional`] for callers on the tokio runtime.
async fn read_optional_async(path: &std::path::Path) -> Option<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!("failed to read {}: {e}", path.display());
            None
        }
    }
}

/// Read a raw content file, returning None if it doesn't exist.
pub fn read_content(id: i64) -> Option<String> {
    read_optional(&content_path(id))
}

/// Async variant of [`read_content`].
pub async fn read_content_async(id: i64) -> Option<String> {
    read_optional_async(&content_path(id)).await
}

/// Write raw content file, creating directories as needed.
pub fn write_content(id: i64, content: &str) -> std::io::Result<()> {
    let path = content_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)
}

/// Delete all cached files for an item. Best-effort — missing files are expected,
/// other errors (permissions, I/O) are logged at warn level.
pub fn delete_item_files(id: i64) {
    try_remove(&content_path(id));
    try_remove(&telegraph_cache_path(id));
}

fn try_remove(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%e, path = %path.display(), "failed to clean up cached file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_path_format() {
        let p = content_path(42);
        let s = p.to_str().unwrap();
        assert!(s.ends_with("042.txt"));
    }

    #[test]
    fn telegraph_cache_path_format() {
        let p = telegraph_cache_path(5);
        assert!(p.to_str().unwrap().ends_with("005-telegraph.json"));
    }

    #[test]
    fn write_and_read_content_direct() {
        let dir = std::env::temp_dir().join(format!("mando-scout-fs-c-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cdir = dir.join("content");
        std::fs::create_dir_all(&cdir).unwrap();

        let path = cdir.join("002.txt");
        std::fs::write(&path, "Raw content here").unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        assert_eq!(data, "Raw content here");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_nonexistent_returns_none() {
        let p = PathBuf::from("/tmp/mando-scout-nonexistent-abc/x.txt");
        assert!(std::fs::read_to_string(&p).ok().is_none());
    }
}
