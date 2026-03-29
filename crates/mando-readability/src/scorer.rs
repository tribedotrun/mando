//! Content-node scoring algorithm (readability.js port).
//!
//! Walks the DOM after cleaning, scores `<p>` elements by text
//! characteristics, and propagates scores to parent/grandparent
//! containers so the best article wrapper bubbles to the top.

use std::collections::HashMap;

use crate::dom::{Dom, NodeId, NodeKind};

/// Regex-like class/id patterns that penalise a container.
const NEGATIVE_PATTERNS: &[&str] = &[
    "comment", "sidebar", "side-bar", "nav", "menu", "footer", "header", "ad", "widget", "social",
    "related", "promo", "sponsor", "banner",
];

/// Patterns that give a container a bonus.
const POSITIVE_PATTERNS: &[&str] = &[
    "article", "content", "post", "entry", "text", "body", "main", "story", "hentry", "blog",
];

/// Score every node that could be an article container.
/// Returns a map from `NodeId` → score.
pub(crate) fn score(dom: &Dom) -> HashMap<NodeId, f64> {
    let mut scores: HashMap<NodeId, f64> = HashMap::new();

    // Find all <p> (and <pre>, <td>) text-bearing elements.
    let candidates = gather_paragraphs(dom, dom.document_id());

    for &pid in &candidates {
        let text = dom.inner_text(pid);
        let text = text.trim();
        // Skip tiny paragraphs.
        if text.len() < 25 {
            continue;
        }

        // Base score: text length / 100, capped at 3.
        let len_score = (text.len() as f64 / 100.0).min(3.0);

        // Comma bonus: +1 per comma.
        let comma_count = text.chars().filter(|&c| c == ',').count() as f64;

        // Long paragraph bonus.
        let long_bonus = if text.len() > 80 { 1.0 } else { 0.0 };

        let para_score = len_score + comma_count + long_bonus;

        // Credit the paragraph itself.
        *scores.entry(pid).or_default() += para_score;

        // Credit the parent.
        let node = dom.node(pid);
        if let Some(parent) = node.parent {
            *scores.entry(parent).or_default() += para_score;

            // Credit the grandparent (half).
            let parent_node = dom.node(parent);
            if let Some(grandparent) = parent_node.parent {
                *scores.entry(grandparent).or_default() += para_score / 2.0;
            }
        }
    }

    // Apply class/id bonuses and penalties to all scored nodes.
    let scored_ids: Vec<NodeId> = scores.keys().copied().collect();
    for id in scored_ids {
        let class_id = dom.class_and_id(id);
        if class_id.is_empty() {
            continue;
        }
        let mut modifier = 0.0_f64;
        for pat in NEGATIVE_PATTERNS {
            if class_id.contains(pat) {
                modifier -= 25.0;
            }
        }
        for pat in POSITIVE_PATTERNS {
            if class_id.contains(pat) {
                modifier += 25.0;
            }
        }
        if modifier != 0.0 {
            *scores.entry(id).or_default() += modifier;
        }
    }

    // Apply element-type weight adjustments.
    for (&id, score) in scores.iter_mut() {
        if let Some(tag) = dom.tag_name(id) {
            match tag.as_str() {
                "div" => {} // neutral
                "article" => *score *= 1.5,
                "section" => *score *= 1.2,
                "pre" | "blockquote" => *score *= 1.1,
                "td" | "th" => *score *= 0.8,
                "address" | "ol" | "ul" | "dl" | "dd" | "dt" | "li" => {
                    *score *= 0.7;
                }
                "form" | "table" => *score *= 0.5,
                _ => {}
            }
        }
    }

    scores
}

/// Collect all `<p>`, `<pre>`, and `<td>` elements in the tree.
fn gather_paragraphs(dom: &Dom, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    walk_paragraphs(dom, root, &mut out);
    out
}

fn walk_paragraphs(dom: &Dom, id: NodeId, out: &mut Vec<NodeId>) {
    let node = dom.node(id);
    if let NodeKind::Element { ref name, .. } = node.kind {
        let tag = name.local.as_ref();
        if matches!(tag, "p" | "pre" | "td") {
            out.push(id);
        }
    }
    for child in node.children {
        walk_paragraphs(dom, child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::parse_document;
    use html5ever::tendril::TendrilSink;

    fn parse(html: &str) -> Dom {
        parse_document(Dom::new(), Default::default()).one(html)
    }

    #[test]
    fn paragraph_scores_propagate_to_parent() {
        let html = r#"<html><body>
            <div id="wrap">
              <p>This paragraph has enough text to score well in the
              readability algorithm, plus commas, here, and there.</p>
            </div>
        </body></html>"#;

        let dom = parse(html);
        crate::cleaner::clean(&dom);
        let scores = score(&dom);

        // The <div id="wrap"> should have a non-zero score.
        let wrap_id = find_by_tag_and_attr(&dom, "div", "id", "wrap");
        assert!(wrap_id.is_some(), "should find the wrapper div");
        let wrap_score = scores.get(&wrap_id.unwrap()).copied().unwrap_or(0.0);
        assert!(wrap_score > 0.0, "wrapper score should be positive");
    }

    #[test]
    fn positive_class_boosts_score() {
        let html = r#"<html><body>
            <div class="article">
              <p>Long content paragraph that scores well in the
              readability algorithm with plenty of text.</p>
            </div>
            <div class="sidebar">
              <p>Long content paragraph that scores well in the
              readability algorithm with plenty of text.</p>
            </div>
        </body></html>"#;

        let dom = parse(html);
        let scores = score(&dom);

        let article_id = find_by_tag_and_attr(&dom, "div", "class", "article");
        let sidebar_id = find_by_tag_and_attr(&dom, "div", "class", "sidebar");

        let a_score = scores.get(&article_id.unwrap()).copied().unwrap_or(0.0);
        let s_score = scores.get(&sidebar_id.unwrap()).copied().unwrap_or(0.0);
        assert!(
            a_score > s_score,
            "article ({a_score}) should outscore sidebar ({s_score})"
        );
    }

    /// Helper: find an element by tag name and a specific attribute value.
    fn find_by_tag_and_attr(dom: &Dom, tag: &str, attr: &str, val: &str) -> Option<NodeId> {
        find_rec(dom, dom.document_id(), tag, attr, val)
    }

    fn find_rec(
        dom: &Dom,
        id: NodeId,
        tag: &str,
        attr_name: &str,
        attr_val: &str,
    ) -> Option<NodeId> {
        if dom.tag_name(id).as_deref() == Some(tag) {
            let attrs = dom.attrs(id);
            if attrs
                .iter()
                .any(|a| a.name.local.as_ref() == attr_name && &*a.value == attr_val)
            {
                return Some(id);
            }
        }
        for child in dom.children(id) {
            if let Some(found) = find_rec(dom, child, tag, attr_name, attr_val) {
                return Some(found);
            }
        }
        None
    }
}
