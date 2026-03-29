//! Article extraction from scored DOM.
//!
//! Picks the top-scoring node and extracts clean text from it.

use std::collections::HashMap;

use crate::dom::{Dom, NodeId, NodeKind};
use crate::ReadabilityError;

/// Find the `<title>` element and return its text, if present.
pub(crate) fn find_title(dom: &Dom) -> Option<String> {
    find_title_rec(dom, dom.document_id())
}

fn find_title_rec(dom: &Dom, id: NodeId) -> Option<String> {
    if dom.tag_name(id).as_deref() == Some("title") {
        let text = dom.inner_text(id).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    for child in dom.children(id) {
        if let Some(t) = find_title_rec(dom, child) {
            return Some(t);
        }
    }
    None
}

/// Select the top-scoring node and extract its content.
///
/// Returns `(html_content, plain_text)`.
pub(crate) fn extract_article(
    dom: &Dom,
    scores: &HashMap<NodeId, f64>,
) -> Result<(String, String), ReadabilityError> {
    if scores.is_empty() {
        return Err(ReadabilityError::NoContent);
    }

    // Pick the highest-scoring node, preferring elements over the
    // document root.
    let best = scores
        .iter()
        .filter(|(&id, _)| id != dom.document_id())
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));

    let &best_id = match best {
        Some((id, _score)) => id,
        None => return Err(ReadabilityError::NoContent),
    };

    // Collect text from the winning subtree.
    let mut html_buf = String::new();
    render_subtree(dom, best_id, &mut html_buf);

    let text = collapse_whitespace(&dom.inner_text(best_id));

    if text.trim().is_empty() {
        return Err(ReadabilityError::NoContent);
    }

    Ok((html_buf, text))
}

/// Render a subtree as simplified HTML (paragraphs only, no attributes).
fn render_subtree(dom: &Dom, id: NodeId, buf: &mut String) {
    let node = dom.node(id);
    match &node.kind {
        NodeKind::Text(t) => buf.push_str(t),
        NodeKind::Element { name, .. } => {
            let tag = name.local.as_ref();
            let is_block = matches!(
                tag,
                "p" | "div"
                    | "article"
                    | "section"
                    | "blockquote"
                    | "pre"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "ul"
                    | "ol"
                    | "li"
            );
            if is_block {
                buf.push_str(&format!("<{tag}>"));
            }
            for &child in &node.children {
                render_subtree(dom, child, buf);
            }
            if is_block {
                buf.push_str(&format!("</{tag}>"));
            }
        }
        _ => {
            for &child in &node.children {
                render_subtree(dom, child, buf);
            }
        }
    }
}

/// Collapse runs of whitespace into a single space and trim.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_whitespace_works() {
        assert_eq!(collapse_whitespace("  hello   world  "), "hello world");
        assert_eq!(collapse_whitespace("a\n\n\tb"), "a b");
        assert_eq!(collapse_whitespace(""), "");
    }

    #[test]
    fn title_extraction() {
        use html5ever::parse_document;
        use html5ever::tendril::TendrilSink;

        let html = "<html><head><title>Test Title</title></head><body></body></html>";
        let dom = parse_document(Dom::new(), Default::default()).one(html);
        assert_eq!(find_title(&dom), Some("Test Title".to_string()));
    }

    #[test]
    fn no_title_returns_none() {
        use html5ever::parse_document;
        use html5ever::tendril::TendrilSink;

        let html = "<html><body><p>no title here</p></body></html>";
        let dom = parse_document(Dom::new(), Default::default()).one(html);
        assert_eq!(find_title(&dom), None);
    }
}
