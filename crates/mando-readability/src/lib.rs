//! HTML article extraction library.
//!
//! Port of the readability.js scoring algorithm — implements only the
//! subset Mando needs: parse HTML, score content nodes, extract the
//! highest-scoring subtree as clean text.

mod cleaner;
mod dom;
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
    /// Clean HTML content of the article container.
    pub content: String,
    /// Plain text content (no HTML tags), whitespace-collapsed.
    pub text_content: String,
}

/// Errors that can occur during extraction.
#[derive(Debug)]
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
    let (content, text_content) = extractor::extract_article(&dom, &scores)?;

    Ok(Article {
        title,
        content,
        text_content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_article_extraction() {
        let html = r#"<html><body>
            <nav>Navigation links here</nav>
            <article>
              <p>This is the main article content with lots of text that
              should be long enough to score well in the readability
              algorithm, providing meaningful content for extraction.</p>
              <p>Second paragraph adds more weight, with commas, here,
              and there, to boost the score even further.</p>
            </article>
            <footer>Copyright 2026</footer>
        </body></html>"#;

        let article = extract(html).expect("should extract");
        assert!(
            article.text_content.contains("main article content"),
            "article text should contain the main content"
        );
        assert!(
            !article.text_content.contains("Navigation links"),
            "article text should not contain nav content"
        );
        assert!(
            !article.text_content.contains("Copyright 2026"),
            "article text should not contain footer content"
        );
    }

    #[test]
    fn extracts_title() {
        let html = "<html><head><title>My Page Title</title></head>\
                     <body><p>Some content here that is long enough.</p></body></html>";
        let article = extract(html).expect("should extract");
        assert_eq!(article.title.as_deref(), Some("My Page Title"));
    }

    #[test]
    fn malformed_html_no_panic() {
        let html = "<div><p>unclosed paragraph<span>also unclosed<div>nested wrong</p>";
        // Should not panic — may return Ok or Err, but must not crash.
        let _ = extract(html);
    }

    #[test]
    fn empty_html_returns_error() {
        assert!(matches!(extract(""), Err(ReadabilityError::EmptyInput)));
        assert!(matches!(extract("   "), Err(ReadabilityError::EmptyInput)));
    }

    #[test]
    fn strips_scripts_and_styles() {
        let html = r#"<html><body>
            <script>alert('xss')</script>
            <style>.evil { display: none }</style>
            <p>This is clean content that should survive the extraction
            process and appear in the final output text.</p>
        </body></html>"#;

        let article = extract(html).expect("should extract");
        assert!(!article.text_content.contains("alert"));
        assert!(!article.text_content.contains(".evil"));
        assert!(article.text_content.contains("clean content"));
    }

    #[test]
    fn nav_footer_only_still_returns_something() {
        // No real article content — just boilerplate. After cleaning,
        // the remaining content is thin but should not panic.
        let html = r#"<html><body>
            <nav><a href="/">Home</a></nav>
            <footer><p>Copyright notice with enough text to be scored
            by the algorithm, even though it is just footer content
            that would normally be stripped away.</p></footer>
        </body></html>"#;

        // This may return Ok (with meager content) or NoContent error.
        let result = extract(html);
        assert!(
            result.is_ok() || matches!(result, Err(ReadabilityError::NoContent)),
            "should return Ok or NoContent, not panic"
        );
    }

    #[test]
    fn scoring_favours_article_over_sidebar() {
        let html = r#"<html><body>
            <div class="sidebar">
              <p>Short sidebar text.</p>
            </div>
            <div class="article">
              <p>This is the primary article content. It contains many
              sentences, commas, and detailed information that makes it
              significantly longer than the sidebar. The readability
              algorithm should score this container higher because of
              the text length and positive class name signal.</p>
              <p>Another paragraph of meaningful content, with more
              commas, adding even more weight to this container.</p>
            </div>
        </body></html>"#;

        let article = extract(html).expect("should extract");
        assert!(
            article.text_content.contains("primary article content"),
            "should pick the article div over the sidebar"
        );
    }
}
