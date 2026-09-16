//! Article extraction from scored DOM.
//!
//! Picks the top-scoring node and extracts clean text from it.

use std::collections::HashMap;

use super::dom::{Dom, NodeId};
use super::ReadabilityError;

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

/// Select the top-scoring node and extract its plain text.
pub(crate) fn extract_article(
    dom: &Dom,
    scores: &HashMap<NodeId, f64>,
) -> Result<String, ReadabilityError> {
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

    let text = collapse_whitespace(&dom.inner_text(best_id));

    if text.trim().is_empty() {
        return Err(ReadabilityError::NoContent);
    }

    Ok(text)
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
