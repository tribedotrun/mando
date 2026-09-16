//! HTML cleanup — remove scripts, styles, nav, and other non-content elements.

use super::dom::Dom;

/// Tags that are always removed before scoring.
const STRIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "iframe", "object", "embed", "applet",
    "link", // <link rel="stylesheet"> etc.
    "meta", "svg", // decorative, not article content
];

/// Tags that are removed because they typically contain boilerplate,
/// not article content.  Note: we strip these *before* scoring so
/// they can't inflate scores of surrounding containers.
const BOILERPLATE_TAGS: &[&str] = &["nav", "footer", "header", "aside", "form"];

/// Walk the DOM and detach unwanted subtrees.
pub(crate) fn clean(dom: &Dom) {
    // Collect ids to remove (breadth-first so we don't revisit
    // children of already-removed nodes).
    let to_remove = collect_removable(dom, dom.document_id());
    for id in to_remove {
        dom.detach(id);
    }
}

/// Recursively collect node ids whose subtrees should be removed.
fn collect_removable(dom: &Dom, id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    for child in dom.children(id) {
        let strip = dom.tag_name(child).is_some_and(|tag| {
            STRIP_TAGS.contains(&tag.as_str()) || BOILERPLATE_TAGS.contains(&tag.as_str())
        });
        if strip {
            result.push(child);
        } else {
            result.extend(collect_removable(dom, child));
        }
    }
    result
}
