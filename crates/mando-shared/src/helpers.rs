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

/// Load a JSON file, returning `T::default()` on missing or corrupt file.
pub fn load_json_file<T: serde::de::DeserializeOwned + Default>(
    path: &std::path::Path,
    module: &str,
) -> T {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(
                module = %module,
                path = %path.display(),
                error = %e,
                "JSON file corrupt — returning default",
            );
            T::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(e) => {
            tracing::warn!(
                module = %module,
                path = %path.display(),
                error = %e,
                "failed to read JSON file — returning default",
            );
            T::default()
        }
    }
}

/// Save a value as pretty-printed JSON, creating parent directories as needed.
pub fn save_json_file<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)?;
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
        let link = pr_html_link("504", Some("tribedotrun/mando-private"));
        assert_eq!(
            link,
            "<a href=\"https://github.com/tribedotrun/mando-private/pull/504\">PR #504</a>"
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
