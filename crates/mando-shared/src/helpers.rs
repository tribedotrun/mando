//! Shared utility functions — PR label normalization, JSON file I/O,
//! and path sanitization.

/// Normalize a PR value to a short `#N` label.
///
/// Accepts either a full GitHub URL (`https://github.com/.../pull/123`)
/// or a short reference (`#123`, `123`). Always returns `#N`.
pub fn pr_short_label(pr: &str) -> String {
    match mando_types::task::extract_pr_number(pr) {
        Some(n) => format!("#{n}"),
        None => pr.to_string(),
    }
}

/// Build a clickable Telegram HTML hyperlink for a PR.
///
/// If `pr` is a full URL, extracts the number and links to the URL.
/// If `pr` is a bare number (or `#N`), constructs a GitHub URL from `github_repo`
/// (e.g. `"owner/repo"`). Returns plain `PR #N` only when no URL can be built.
pub fn pr_html_link(pr: &str, github_repo: Option<&str>) -> String {
    let num = match mando_types::task::extract_pr_number(pr) {
        Some(n) => n,
        None => return crate::telegram_format::escape_html(pr),
    };
    let label = crate::telegram_format::escape_html(&format!("PR #{num}"));
    // Full URL already present — use it directly.
    if pr.starts_with("http") {
        return format!(
            "<a href=\"{}\">{label}</a>",
            crate::telegram_format::escape_html(pr),
        );
    }
    // Construct URL from github_repo if available.
    if let Some(repo) = github_repo {
        let url = format!("https://github.com/{repo}/pull/{num}");
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
    fn pr_short_label_from_url() {
        assert_eq!(
            pr_short_label("https://github.com/org/repo/pull/123"),
            "#123"
        );
    }

    #[test]
    fn pr_short_label_from_hash() {
        assert_eq!(pr_short_label("#42"), "#42");
    }

    #[test]
    fn pr_short_label_from_bare_number() {
        assert_eq!(pr_short_label("99"), "#99");
    }

    #[test]
    fn pr_html_link_full_url() {
        let link = pr_html_link("https://github.com/org/repo/pull/123", None);
        assert_eq!(
            link,
            "<a href=\"https://github.com/org/repo/pull/123\">PR #123</a>"
        );
    }

    #[test]
    fn pr_html_link_bare_number_with_repo() {
        let link = pr_html_link("504", Some("tribedotrun/mando"));
        assert_eq!(
            link,
            "<a href=\"https://github.com/tribedotrun/mando/pull/504\">PR #504</a>"
        );
    }

    #[test]
    fn pr_html_link_hash_ref_with_repo() {
        let link = pr_html_link("#42", Some("acme/widgets"));
        assert_eq!(
            link,
            "<a href=\"https://github.com/acme/widgets/pull/42\">PR #42</a>"
        );
    }

    #[test]
    fn pr_html_link_bare_number_no_repo() {
        let link = pr_html_link("99", None);
        assert_eq!(link, "PR #99");
    }
}
