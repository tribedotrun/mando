//! HTTP content fetching — fetch a URL and extract article text.
//!
//! Strategy chain:
//! 1. Twitter/X URLs → oEmbed API (tweet text + resolve linked URLs)
//! 2. Readability (reqwest + HTML extraction) — fast, free, static pages
//! 3. Firecrawl fallback — JS-rendered pages

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::runtime::firecrawl;

/// Shared HTTP client for content fetching — avoids per-request TLS handshakes.
fn shared_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/122.0.0.0 Safari/537.36",
            )
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Shared no-redirect client for URL resolution (e.g. t.co links).
fn shared_redirect_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build redirect client")
    })
}

/// Minimum characters for a valid content extraction.
const MIN_CONTENT_CHARS: usize = 200;

/// Fetch and extract readable article content from a URL.
pub async fn fetch_content(url: &str) -> Result<String> {
    // Tweet status URLs: try oEmbed, degrade gracefully if it fails.
    // Firecrawl blocks X, so there's no viable fallback — return a stub.
    if is_tweet_status_url(url) {
        match fetch_twitter(url).await {
            Ok(content) => return Ok(content),
            Err(e) => {
                warn!(url, error = %e, "oEmbed failed, returning stub");
                return Ok(format!("Tweet (content unavailable): {url}"));
            }
        }
    }

    // YouTube URLs → transcript extraction via yt-dlp.
    if is_youtube_url(url) {
        return crate::runtime::youtube::extract_youtube_transcript(url).await;
    }

    // Standard path: readability → firecrawl fallback
    match try_readability(url).await {
        Ok(content) => Ok(content),
        Err(e) => {
            info!(url, error = %e, "readability failed, trying firecrawl");
            firecrawl_fallback(url).await
        }
    }
}

fn is_youtube_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("youtube.com/watch")
        || lower.contains("youtu.be/")
        || lower.contains("youtube.com/shorts/")
}

fn is_twitter_url_lower(lower: &str) -> bool {
    lower.contains("://x.com/")
        || lower.contains("://www.x.com/")
        || lower.contains("://twitter.com/")
        || lower.contains("://www.twitter.com/")
}

fn is_twitter_url(url: &str) -> bool {
    is_twitter_url_lower(&url.to_lowercase())
}

/// True for tweet/status URLs only — NOT for x.com articles or other content.
fn is_tweet_status_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    is_twitter_url_lower(&lower) && lower.contains("/status/")
}

/// Fetch tweet via oEmbed, resolve embedded links, fetch linked content.
async fn fetch_twitter(url: &str) -> Result<String> {
    info!(url, "twitter URL detected, using oEmbed");

    let oembed_url = format!(
        "https://publish.twitter.com/oembed?url={}&omit_script=true",
        urlencoding::encode(url)
    );

    let resp = shared_client()
        .get(&oembed_url)
        .send()
        .await
        .context("oEmbed request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("oEmbed returned HTTP {}", resp.status());
    }

    let oembed: serde_json::Value = resp.json().await.context("oEmbed parse error")?;

    let author = oembed["author_name"].as_str().unwrap_or("Unknown");
    let html = oembed["html"].as_str().unwrap_or("");

    // Extract text from the blockquote HTML
    let tweet_text = extract_tweet_text(html);

    // Find t.co links and resolve them — fetch the actual linked content
    let links = extract_tco_links(html);
    let mut resolved_content = String::new();

    for link in &links {
        if let Ok(resolved) = resolve_tco(link).await {
            // Skip all X/Twitter links — firecrawl blocks the domain and
            // readability can't render JS. Include the URL as a reference instead.
            if is_twitter_url(&resolved) {
                info!(tco = link, resolved = %resolved, "skipping X URL (unfetchable)");
                if resolved_content.is_empty() {
                    resolved_content = format!("Linked: {resolved}");
                }
                continue;
            }
            info!(tco = link, resolved = %resolved, "resolved t.co link");
            // Try fetching the linked content (readability → firecrawl)
            if let Ok(content) = try_fetch_linked(&resolved).await {
                resolved_content = content;
                break;
            }
        }
    }

    // Build combined content — always return what we have for tweets.
    // Even short tweet text (e.g. "check this article") is processable by the AI scorer.
    let mut result = format!("Tweet by @{author}:\n\n{tweet_text}\n");

    if !resolved_content.is_empty() {
        result.push_str("\n---\n\nLinked content:\n\n");
        result.push_str(&resolved_content);
    }

    if tweet_text.is_empty() && resolved_content.is_empty() {
        anyhow::bail!("oEmbed returned empty tweet text for {url}");
    }

    Ok(result)
}

/// Try readability then firecrawl on a resolved linked URL.
async fn try_fetch_linked(url: &str) -> Result<String> {
    match try_readability(url).await {
        Ok(content) => Ok(content),
        Err(e) => {
            info!(url, error = %e, "readability failed on linked URL, trying firecrawl");
            firecrawl_fallback(url).await
        }
    }
}

/// Extract readable text from Twitter blockquote HTML.
fn extract_tweet_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }

    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Extract t.co links from HTML.
fn extract_tco_links(html: &str) -> Vec<String> {
    let mut links = Vec::new();
    let prefix = "https://t.co/";
    let mut search_from = 0;

    while let Some(pos) = html[search_from..].find(prefix) {
        let start = search_from + pos;
        let end = html[start..]
            .find(['"', '<', ' '])
            .map(|e| start + e)
            .unwrap_or(html.len());
        let link = &html[start..end];
        if !links.contains(&link.to_string()) {
            links.push(link.to_string());
        }
        search_from = end;
    }

    links
}

/// Resolve a t.co shortened URL to its final destination.
async fn resolve_tco(tco_url: &str) -> Result<String> {
    let resp = shared_redirect_client().head(tco_url).send().await?;

    resp.headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .with_context(|| format!("no redirect location for {tco_url}"))
}

/// Try readability-based extraction (fast path).
async fn try_readability(url: &str) -> Result<String> {
    let html = fetch_raw(url).await?;
    match mando_readability::extract(&html) {
        Ok(article) if article.text_content.len() >= MIN_CONTENT_CHARS => Ok(article.text_content),
        Ok(article) => {
            info!(
                url,
                chars = article.text_content.len(),
                "readability extraction short, returning as-is"
            );
            Ok(article.text_content)
        }
        Err(e) => Err(anyhow::anyhow!("readability extraction failed: {e}")),
    }
}

/// Firecrawl fallback for JS-rendered pages.
async fn firecrawl_fallback(url: &str) -> Result<String> {
    let content = firecrawl::scrape(url)
        .await
        .with_context(|| format!("firecrawl scrape failed for {url}"))?;
    if content.len() < MIN_CONTENT_CHARS {
        anyhow::bail!(
            "firecrawl content too short ({} chars) for {url}",
            content.len()
        );
    }
    info!(url, chars = content.len(), "firecrawl extraction succeeded");
    Ok(content)
}

/// Fetch raw HTML from a URL.
pub async fn fetch_raw(url: &str) -> Result<String> {
    let resp = shared_client()
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.5")
        .send()
        .await
        .with_context(|| format!("HTTP GET failed for {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status} for {url}");
    }

    resp.text()
        .await
        .with_context(|| format!("reading response body from {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_twitter_urls() {
        assert!(is_twitter_url("https://x.com/user/status/123"));
        assert!(is_twitter_url("https://twitter.com/user/status/123"));
        assert!(!is_twitter_url("https://example.com/page"));
        assert!(!is_twitter_url("https://fox.com/article"));
        assert!(!is_twitter_url("https://vox.com/feature/story"));
    }

    #[test]
    fn detect_tweet_status_vs_article() {
        assert!(is_tweet_status_url("https://x.com/user/status/123"));
        assert!(!is_tweet_status_url("https://x.com/i/article/123"));
        assert!(!is_tweet_status_url("https://x.com/user"));
    }

    #[test]
    fn extract_tco_links_from_html() {
        let html = r#"<a href="https://t.co/abc123">link</a> and <a href="https://t.co/def456">another</a>"#;
        let links = extract_tco_links(html);
        assert_eq!(links, vec!["https://t.co/abc123", "https://t.co/def456"]);
    }

    #[test]
    fn extract_tweet_text_basic() {
        let html =
            r#"<blockquote><p>Hello world <a href="https://t.co/abc">link</a></p></blockquote>"#;
        let text = extract_tweet_text(html);
        assert!(text.contains("Hello world"));
        assert!(text.contains("link"));
    }
}
