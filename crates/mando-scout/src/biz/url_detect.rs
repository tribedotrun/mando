//! URL classification — determine the type of a scout item from its URL.

/// The type of content at a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlType {
    YouTube,
    ArXiv,
    Blog,
    Repo,
    Unknown,
}

impl UrlType {
    /// String label matching the Python `detect_url_type` return values.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::ArXiv => "arxiv",
            Self::Repo => "github",
            Self::Blog => "blog",
            Self::Unknown => "other",
        }
    }
}

/// Classify a URL into a content type.
///
/// Rules (matching Python `detect_url_type`):
/// - YouTube: hostname is youtube.com, www.youtube.com, m.youtube.com, or youtu.be
/// - ArXiv: hostname is arxiv.org, www.arxiv.org, or ar5iv.labs.arxiv.org
/// - Repo: hostname contains "github.com" and path matches owner/repo pattern
/// - Blog: everything else
pub fn classify_url(url: &str) -> UrlType {
    let host = extract_host(url);

    // YouTube
    if matches!(
        host.as_str(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be"
    ) {
        return UrlType::YouTube;
    }

    // ArXiv
    if matches!(
        host.as_str(),
        "arxiv.org" | "www.arxiv.org" | "ar5iv.labs.arxiv.org"
    ) {
        return UrlType::ArXiv;
    }

    // GitHub repo: host contains "github.com" and path has owner/repo pattern
    if host.contains("github.com") {
        if is_repo_path(url) {
            return UrlType::Repo;
        }
        // GitHub page but not a repo (e.g. gist, profile)
        return UrlType::Blog;
    }

    UrlType::Blog
}

/// Derive a source label from URL and type (matches Python `derive_source_label`).
pub fn derive_source_label(url: &str, url_type: &str) -> String {
    let host = extract_host(url);

    if url_type == "youtube" {
        return "YouTube".into();
    }
    if url_type == "arxiv" {
        return "arXiv".into();
    }
    if url_type == "github" {
        let path = extract_path(url);
        let trimmed = path.trim_matches('/');
        if let Some(owner) = trimmed.split('/').next() {
            if !owner.is_empty() {
                return owner.into();
            }
        }
        return "GitHub".into();
    }

    // Blogs/other: use domain, strip www.
    let label = host.strip_prefix("www.").unwrap_or(&host);
    if label.is_empty() {
        "web".into()
    } else {
        label.into()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract lowercase hostname from a URL.
fn extract_host(url: &str) -> String {
    // Skip scheme
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    // Take up to first / or ?
    let host_part = after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .split('?')
        .next()
        .unwrap_or(after_scheme)
        .split('#')
        .next()
        .unwrap_or(after_scheme);
    // Strip port
    let host = if let Some(pos) = host_part.rfind(':') {
        // Only strip if what follows looks like a port number
        let after = &host_part[pos + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            &host_part[..pos]
        } else {
            host_part
        }
    } else {
        host_part
    };
    // Strip userinfo (user@host)
    let host = if let Some(pos) = host.find('@') {
        &host[pos + 1..]
    } else {
        host
    };
    host.to_lowercase()
}

/// Extract path portion of a URL.
fn extract_path(url: &str) -> String {
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    if let Some(slash_pos) = after_scheme.find('/') {
        let path_and_rest = &after_scheme[slash_pos..];
        // Strip query and fragment
        let path = path_and_rest.split('?').next().unwrap_or(path_and_rest);
        let path = path.split('#').next().unwrap_or(path);
        path.to_string()
    } else {
        "/".into()
    }
}

/// Check if a github.com URL has an owner/repo path pattern.
/// Must have exactly two non-empty path segments (owner and repo).
fn is_repo_path(url: &str) -> bool {
    let path = extract_path(url);
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    // owner/repo = exactly 2 segments, or owner/repo/... with more
    // Python: "github.com" in host → always returns "github" regardless of path shape
    // Actually re-reading the Python: it just checks host == github.com → "github"
    // But the spec says: Repo = contains "github.com" followed by exactly owner/repo
    // Let's follow the spec: exactly owner/repo pattern (2 segments)
    parts.len() == 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_standard() {
        assert_eq!(
            classify_url("https://www.youtube.com/watch?v=abc123"),
            UrlType::YouTube
        );
    }

    #[test]
    fn youtube_short() {
        assert_eq!(classify_url("https://youtu.be/abc123"), UrlType::YouTube);
    }

    #[test]
    fn youtube_mobile() {
        assert_eq!(
            classify_url("https://m.youtube.com/watch?v=abc123"),
            UrlType::YouTube
        );
    }

    #[test]
    fn arxiv_standard() {
        assert_eq!(
            classify_url("https://arxiv.org/abs/2301.12345"),
            UrlType::ArXiv
        );
    }

    #[test]
    fn arxiv_www() {
        assert_eq!(
            classify_url("https://www.arxiv.org/abs/1706.03762"),
            UrlType::ArXiv
        );
    }

    #[test]
    fn arxiv_ar5iv() {
        assert_eq!(
            classify_url("https://ar5iv.labs.arxiv.org/html/1706.03762v7"),
            UrlType::ArXiv
        );
    }

    #[test]
    fn github_repo() {
        assert_eq!(
            classify_url("https://github.com/rust-lang/rust"),
            UrlType::Repo
        );
    }

    #[test]
    fn github_not_repo() {
        // Profile page is not a repo path (only 1 segment)
        assert_eq!(classify_url("https://github.com/rust-lang"), UrlType::Blog);
    }

    #[test]
    fn github_deep_path_not_repo() {
        // 3+ segments is not exactly owner/repo
        assert_eq!(
            classify_url("https://github.com/rust-lang/rust/issues"),
            UrlType::Blog
        );
    }

    #[test]
    fn blog_standard() {
        assert_eq!(classify_url("https://example.com/blog/post"), UrlType::Blog);
    }

    #[test]
    fn url_type_as_str() {
        assert_eq!(UrlType::YouTube.as_str(), "youtube");
        assert_eq!(UrlType::ArXiv.as_str(), "arxiv");
        assert_eq!(UrlType::Repo.as_str(), "github");
        assert_eq!(UrlType::Blog.as_str(), "blog");
    }

    #[test]
    fn derive_source_youtube() {
        assert_eq!(
            derive_source_label("https://youtube.com/watch?v=x", "youtube"),
            "YouTube"
        );
    }

    #[test]
    fn derive_source_arxiv() {
        assert_eq!(
            derive_source_label("https://arxiv.org/abs/123", "arxiv"),
            "arXiv"
        );
    }

    #[test]
    fn derive_source_github() {
        assert_eq!(
            derive_source_label("https://github.com/tokio-rs/tokio", "github"),
            "tokio-rs"
        );
    }

    #[test]
    fn derive_source_blog_strips_www() {
        assert_eq!(
            derive_source_label("https://www.example.com/post", "blog"),
            "example.com"
        );
    }

    #[test]
    fn derive_source_blog_no_www() {
        assert_eq!(
            derive_source_label("https://blog.example.com/post", "blog"),
            "blog.example.com"
        );
    }
}
