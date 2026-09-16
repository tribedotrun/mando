//! HTML article extraction library.
//!
//! Port of the readability.js scoring algorithm — implements only the
//! subset Mando needs: parse HTML, score content nodes, extract the
//! highest-scoring subtree as clean text.

pub(crate) mod cleaner;
pub(crate) mod dom;
mod extractor;
mod scorer;

use dom::Dom;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;

/// A successfully extracted article.
#[derive(Debug, Clone)]
pub struct Article {
    /// Document title from `<title>` or `<h1>`, if found.
    pub title: Option<String>,
    /// Plain text content (no HTML tags), whitespace-collapsed.
    pub text_content: String,
}

/// Errors that can occur during extraction.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadabilityError {
    /// The input was empty or contained no parseable content.
    EmptyInput,
    /// Parsing succeeded but no scoreable content was found.
    NoContent,
}

impl std::fmt::Display for ReadabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty or blank HTML input"),
            Self::NoContent => write!(f, "no article content found"),
        }
    }
}

impl std::error::Error for ReadabilityError {}

/// Extract an article from raw HTML.
///
/// Returns the highest-scoring content subtree as both clean HTML and
/// plain text.  Scripts, styles, nav, footer, and other boilerplate
/// elements are stripped before scoring.
pub fn extract(html: &str) -> Result<Article, ReadabilityError> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Err(ReadabilityError::EmptyInput);
    }

    let dom = parse_document(Dom::new(), Default::default()).one(trimmed);

    // Step 1: extract title before cleaning.
    let title = extractor::find_title(&dom);

    // Step 2: remove unwanted elements (script, style, nav, etc.).
    cleaner::clean(&dom);

    // Step 3: score remaining nodes.
    let scores = scorer::score(&dom);

    // Step 4: pick the top-scoring node and extract its text.
    let text_content = extractor::extract_article(&dom, &scores)?;

    Ok(Article {
        title,
        text_content,
    })
}
