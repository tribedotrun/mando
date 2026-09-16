//! Formatting helpers for scout summaries and prompts.

/// Format a list of strings as a markdown bullet list (one `- ` per item).
pub fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n")
}
