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
}
