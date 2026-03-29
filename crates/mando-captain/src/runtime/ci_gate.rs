//! CI gate operations — PR URL parsing utilities.

/// Parse a PR URL like "https://github.com/owner/repo/pull/123" into (repo, pr_number).
pub(crate) fn parse_pr_url(pr_url: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = pr_url.trim_end_matches('/').split('/').collect();
    if parts.len() >= 5 {
        let repo = format!("{}/{}", parts[parts.len() - 4], parts[parts.len() - 3]);
        let num = parts[parts.len() - 1].to_string();
        Some((repo, num))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_url_valid() {
        let (repo, num) = parse_pr_url("https://github.com/acme/widgets/pull/116").unwrap();
        assert_eq!(repo, "acme/widgets");
        assert_eq!(num, "116");
    }

    #[test]
    fn parse_pr_url_trailing_slash() {
        let (repo, num) = parse_pr_url("https://github.com/acme/widgets/pull/42/").unwrap();
        assert_eq!(repo, "acme/widgets");
        assert_eq!(num, "42");
    }

    #[test]
    fn parse_pr_url_invalid() {
        assert!(parse_pr_url("not-a-url").is_none());
        assert!(parse_pr_url("#123").is_none());
    }
}
