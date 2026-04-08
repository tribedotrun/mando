//! Shared utility functions — PR label normalization, JSON file I/O,
//! and path sanitization.

/// Format a PR number as a short `#N` label.
///
/// Delegates to `mando_types::task::pr_label`.
pub fn pr_short_label(pr_number: i64) -> String {
    mando_types::task::pr_label(pr_number)
}

/// Build a clickable Telegram HTML hyperlink for a PR number.
///
/// Constructs a GitHub URL from `github_repo` (e.g. `"owner/repo"`) when
/// available. Returns plain `PR #N` when no URL can be built.
pub fn pr_html_link(pr_number: i64, github_repo: Option<&str>) -> String {
    let label = crate::telegram_format::escape_html(&format!("PR #{pr_number}"));
    if let Some(repo) = github_repo {
        let url = mando_types::task::pr_url(repo, pr_number);
        return format!(
            "<a href=\"{}\">{label}</a>",
            crate::telegram_format::escape_html(&url),
        );
    }
    // No URL possible — plain text fallback.
    label
}

/// Sanitize an ID for safe use in file paths (prevent path traversal).
pub fn sanitize_path_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Load a JSON file, returning `T::default()` on a missing file and an error
/// on any other failure (IO error or parse error).
///
/// Missing is treated as a healthy fresh-state because most callers use this
/// for state files that only exist after the first save. Corrupt files are
/// errors so the caller can decide whether to bail, rename-aside, or retry.
/// Callers that want the "always return a value, log on corruption" behavior
/// can chain `.unwrap_or_default()` at the call site.
///
/// The `_module` parameter is kept for API compatibility but is no longer
/// interpolated into error strings; use structured logging at the call site
/// to capture module context.
pub fn load_json_file<T: serde::de::DeserializeOwned + Default>(
    path: &std::path::Path,
    _module: &str,
) -> Result<T, crate::error::SharedError> {
    use crate::error::SharedError;
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| SharedError::JsonParse {
            path: path.to_path_buf(),
            source: e,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(SharedError::Io {
            op: "read".into(),
            path: path.to_path_buf(),
            source: e,
        }),
    }
}

/// Save a value as pretty-printed JSON atomically.
///
/// Writes to a per-call unique temp file alongside the target, fsyncs, then
/// renames onto `path` so concurrent readers never see a partially written
/// file and concurrent writers cannot race on the same temp pathname. On
/// rename failure, removes the temp file to avoid leaving orphans on disk.
pub fn save_json_file<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), crate::error::SharedError> {
    use crate::error::SharedError;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SharedError::Io {
            op: "create_dir_all".into(),
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let json = serde_json::to_string_pretty(value)?;

    // Unique temp name per write: PID + monotonic counter + nanos. Two
    // concurrent writers to the same path each get a distinct temp file,
    // so neither can rename the other's in-flight file out from under it.
    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!(
        "{}.tmp.{}.{}.{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("json"),
        std::process::id(),
        seq,
        nanos,
    ));

    let mut f = std::fs::File::create(&tmp).map_err(|e| SharedError::Io {
        op: "create".into(),
        path: tmp.clone(),
        source: e,
    })?;
    f.write_all(json.as_bytes()).map_err(|e| SharedError::Io {
        op: "write".into(),
        path: tmp.clone(),
        source: e,
    })?;
    f.sync_all().map_err(|e| SharedError::Io {
        op: "fsync".into(),
        path: tmp.clone(),
        source: e,
    })?;
    drop(f);
    // Clean up the temp file if the rename fails; otherwise repeated save
    // failures leave stale `.tmp.*` files cluttering the state directory.
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(SharedError::Io {
            op: "rename".into(),
            path: path.to_path_buf(),
            source: e,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_short_label_formats_number() {
        assert_eq!(pr_short_label(123), "#123");
    }

    #[test]
    fn pr_html_link_with_repo() {
        let link = pr_html_link(504, Some("tribedotrun/mando"));
        assert_eq!(
            link,
            "<a href=\"https://github.com/tribedotrun/mando/pull/504\">PR #504</a>"
        );
    }

    #[test]
    fn pr_html_link_no_repo() {
        let link = pr_html_link(99, None);
        assert_eq!(link, "PR #99");
    }
}
