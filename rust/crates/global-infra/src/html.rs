use std::sync::LazyLock;

use regex::Regex;

/// Escape HTML special characters for safe embedding in HTML text.
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Matches `PR #123` or `PR#123` (case-insensitive, requires PR prefix).
static PR_REF_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"(?i)\bPR\s*#(\d+)") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("PR_REF_RE compilation failed", e),
});

/// Scan `text` for PR references and replace each with a clickable HTML hyperlink.
pub fn linkify_pr_refs(text: &str, repo_slug: &str) -> String {
    let repo_name = repo_slug.rsplit('/').next().unwrap_or(repo_slug);
    PR_REF_RE
        .replace_all(text, |caps: &regex::Captures| {
            let num = &caps[1];
            let url = format!("https://github.com/{repo_slug}/pull/{num}");
            format!(
                "<a href=\"{url}\">{repo} PR #{num}</a>",
                repo = escape_html(repo_name),
            )
        })
        .into_owned()
}
