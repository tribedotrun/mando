//! HTML cleanup — remove scripts, styles, nav, and other non-content elements.

use crate::dom::Dom;

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
    let children = dom.children(id);
    for child in children {
        if should_remove(dom, child) {
            result.push(child);
        } else {
            result.extend(collect_removable(dom, child));
        }
    }
    result
}

fn should_remove(dom: &Dom, id: usize) -> bool {
    if let Some(tag) = dom.tag_name(id) {
        let tag = tag.as_str();
        if STRIP_TAGS.contains(&tag) || BOILERPLATE_TAGS.contains(&tag) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::parse_document;
    use html5ever::tendril::TendrilSink;

    fn parse(html: &str) -> Dom {
        parse_document(Dom::new(), Default::default()).one(html)
    }

    fn all_text(dom: &Dom) -> String {
        dom.inner_text(dom.document_id())
    }

    #[test]
    fn removes_script_tags() {
        let dom = parse("<html><body><script>evil()</script><p>ok</p></body></html>");
        clean(&dom);
        let text = all_text(&dom);
        assert!(!text.contains("evil"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn removes_style_tags() {
        let dom = parse("<html><body><style>.x{}</style><p>ok</p></body></html>");
        clean(&dom);
        assert!(!all_text(&dom).contains(".x"));
    }

    #[test]
    fn removes_nav_and_footer() {
        let dom =
            parse("<html><body><nav>links</nav><p>article</p><footer>copy</footer></body></html>");
        clean(&dom);
        let text = all_text(&dom);
        assert!(!text.contains("links"));
        assert!(!text.contains("copy"));
        assert!(text.contains("article"));
    }
}
