//! Summary formatting and slug generation for scout items.

/// Format a list of strings as a markdown bullet list (one `- ` per item).
pub fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate a filesystem-safe slug from a title.
///
/// Matches the Python `slugify` function: lowercase, strip non-word chars,
/// collapse hyphens, trim, truncate to 60 chars.
pub fn slugify_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut slug = String::with_capacity(lower.len());

    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            slug.push(ch);
        } else if ch.is_whitespace() || ch == '_' {
            slug.push('-');
        }
        // Other chars are dropped
    }

    // Collapse consecutive hyphens
    let mut collapsed = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for ch in slug.chars() {
        if ch == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(ch);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens and truncate to 60
    let trimmed = collapsed.trim_matches('-');
    if trimmed.len() <= 60 {
        trimmed.to_string()
    } else {
        trimmed[..60].trim_end_matches('-').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify_title("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(
            slugify_title("What's New in Rust 2026?"),
            "whats-new-in-rust-2026"
        );
    }

    #[test]
    fn slugify_collapse_hyphens() {
        assert_eq!(slugify_title("a --- b"), "a-b");
    }

    #[test]
    fn slugify_truncate() {
        let long_title = "a".repeat(100);
        let slug = slugify_title(&long_title);
        assert!(slug.len() <= 60);
    }

    #[test]
    fn slugify_empty() {
        assert_eq!(slugify_title(""), "");
    }

    #[test]
    fn slugify_unicode_stripped() {
        assert_eq!(slugify_title("cafe"), "cafe");
    }
}
