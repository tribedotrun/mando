//! Formatting helpers for scout summaries and prompts.

/// Format a list of strings as a markdown bullet list (one `- ` per item).
pub fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullet_list_joins_with_dashes() {
        let items = vec!["foo".into(), "bar".into()];
        assert_eq!(bullet_list(&items), "- foo\n- bar");
    }

    #[test]
    fn bullet_list_empty() {
        assert_eq!(bullet_list(&[]), "");
    }
}
