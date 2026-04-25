//! HTTP content fetching — fetch a URL and extract article text.
//!
//! Strategy chain:
//! 1. Twitter/X URLs → oEmbed API (tweet text + resolve linked URLs)
//! 2. YouTube URLs → yt-dlp (transcript + info.json metadata)
//! 3. Readability (reqwest + HTML extraction) — fast, free, static pages
//! 4. Firecrawl fallback — JS-rendered pages
//!
//! Every path populates the same `FetchedContent` shape so the caller never
//! has to guess whether a title/date is available.

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::firecrawl;
use super::metadata_probe::{self, HtmlMetadata};

/// Unified result of a content fetch. Every strategy below fills what it can
/// and leaves the rest as `None` so the caller never needs to probe a
/// side-channel.
#[derive(Debug)]
pub struct FetchedContent {
    pub text: String,
    pub extracted_title: Option<String>,
    /// Publication date normalized to `YYYY-MM-DD`.
    pub extracted_date: Option<String>,
}

/// Shared HTTP client for content fetching — governed in `global-net` so
/// all outbound HTTP is centralized.
fn shared_client() -> std::sync::Arc<reqwest::Client> {
    global_net::http::html_fetch_client()
}

/// Shared no-redirect client for URL resolution (e.g. t.co links).
fn shared_redirect_client() -> std::sync::Arc<reqwest::Client> {
    global_net::http::html_fetch_no_redirect_client()
}

/// Minimum characters for a valid content extraction.
const MIN_CONTENT_CHARS: usize = 200;

/// Fetch and extract readable article content plus any available metadata.
pub async fn fetch_content(url: &str) -> Result<FetchedContent> {
    // Tweet status URLs: oEmbed is the only viable path. Firecrawl blocks X,
    // so there is no fallback — return the error instead of a stub that would
    // be scored and summarized as if it were real content.
    if is_tweet_status_url(url) {
        return fetch_twitter(url)
            .await
            .with_context(|| format!("oEmbed failed for tweet {url}"));
    }

    // YouTube URLs → transcript via yt-dlp → firecrawl fallback.
    // Return the underlying error if both fail; never synthesize a stub.
    if is_youtube_url(url) {
        match super::youtube::extract_youtube_transcript(url).await {
            Ok(yt) => {
                return Ok(FetchedContent {
                    text: yt.transcript,
                    extracted_title: yt.title,
                    extracted_date: yt.publish_date,
                });
            }
            Err(e) => {
                warn!(url, error = %e, "yt-dlp failed, trying firecrawl");
                return firecrawl_fallback(url).await.with_context(|| {
                    format!("yt-dlp and firecrawl both failed for YouTube {url}")
                });
            }
        }
    }

    // Standard path: readability → firecrawl fallback.
    match try_readability(url).await {
        Ok(result) => Ok(result),
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

fn is_twitter_url(url: &str) -> bool {
    const TWITTER_HOSTS: &[&str] = &["x.com", "www.x.com", "twitter.com", "www.twitter.com"];
    let Some(host) = extract_host(url) else {
        return false;
    };
    TWITTER_HOSTS.contains(&host.to_lowercase().as_str())
}

/// Extract the host component from an `http(s)://host/...` URL. Returns `None`
/// for relative URLs or anything without a scheme.
fn extract_host(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let host = &after_scheme[..end];
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// True for tweet/status URLs only — NOT for x.com articles or other content.
pub fn is_tweet_status_url(url: &str) -> bool {
    is_twitter_url(url) && url.to_lowercase().contains("/status/")
}

/// Fetch tweet via oEmbed, resolve embedded links, fetch linked content.
async fn fetch_twitter(url: &str) -> Result<FetchedContent> {
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

    let tweet_text = extract_tweet_text(html);

    // Find t.co links and resolve them — fetch the actual linked content.
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

    Ok(FetchedContent {
        text: result,
        extracted_title: Some(tweet_title(author, &tweet_text)),
        extracted_date: metadata_probe::snowflake_date_from_tweet_url(url),
    })
}

/// Build a deterministic title for a tweet: "Tweet by @author: first-bit".
fn tweet_title(author: &str, text: &str) -> String {
    let snippet: String = text.chars().take(80).collect();
    let snippet = snippet.trim();
    if snippet.is_empty() {
        format!("Tweet by @{author}")
    } else {
        format!("Tweet by @{author}: {snippet}")
    }
}

/// Try readability then firecrawl on a resolved linked URL. Returns plain
/// text only — linked-content metadata is not propagated.
async fn try_fetch_linked(url: &str) -> Result<String> {
    match try_readability(url).await {
        Ok(result) => Ok(result.text),
        Err(e) => {
            info!(url, error = %e, "readability failed on linked URL, trying firecrawl");
            firecrawl_fallback(url).await.map(|f| f.text)
        }
    }
}

/// Extract readable text from Twitter blockquote HTML.
fn extract_tweet_text(html: &str) -> String {
    let text = super::strip_html_tags(html);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// Try readability-based extraction (fast path). Probes the raw HTML for
/// title and publish date BEFORE readability strips meta/time/JSON-LD.
async fn try_readability(url: &str) -> Result<FetchedContent> {
    let html = fetch_raw(url).await?;
    let HtmlMetadata {
        title,
        date_published,
    } = metadata_probe::probe_html(&html);
    let article = global_net::readability::extract(&html)
        .map_err(|e| anyhow::anyhow!("readability extraction failed: {e}"))?;
    if article.text_content.len() < MIN_CONTENT_CHARS {
        info!(
            url,
            chars = article.text_content.len(),
            "readability extraction short, returning as-is"
        );
    }
    Ok(FetchedContent {
        text: article.text_content,
        // Readability's own title is a reasonable last resort; raw-HTML probe
        // is our first choice because it reads og:title / <title> directly.
        extracted_title: title.or(article.title),
        extracted_date: date_published,
    })
}

/// Firecrawl fallback for JS-rendered pages.
async fn firecrawl_fallback(url: &str) -> Result<FetchedContent> {
    let result = firecrawl::scrape(url)
        .await
        .with_context(|| format!("firecrawl scrape failed for {url}"))?;
    if result.markdown.len() < MIN_CONTENT_CHARS {
        anyhow::bail!(
            "firecrawl content too short ({} chars) for {url}",
            result.markdown.len()
        );
    }
    info!(
        url,
        chars = result.markdown.len(),
        "firecrawl extraction succeeded"
    );
    Ok(FetchedContent {
        text: result.markdown,
        extracted_title: result.title,
        extracted_date: result.date_published,
    })
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

    #[test]
    fn tweet_title_with_text() {
        let t = tweet_title("alice", "check this amazing thread about rust async");
        assert_eq!(
            t,
            "Tweet by @alice: check this amazing thread about rust async"
        );
    }

    #[test]
    fn tweet_title_empty_text_falls_back_to_author() {
        assert_eq!(tweet_title("bob", ""), "Tweet by @bob");
        assert_eq!(tweet_title("bob", "   "), "Tweet by @bob");
    }

    #[test]
    fn tweet_title_truncates_long_text() {
        let long = "a".repeat(200);
        let t = tweet_title("u", &long);
        assert!(t.len() <= "Tweet by @u: ".len() + 80);
    }
}
