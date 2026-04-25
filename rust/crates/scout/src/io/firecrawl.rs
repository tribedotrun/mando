//! Firecrawl integration — web scraping via Firecrawl v1 API.
//!
//! API key read from `FIRECRAWL_API_KEY` env var (injected via config.json env section).

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::metadata_probe;

const FIRECRAWL_SCRAPE_URL: &str = "https://api.firecrawl.dev/v1/scrape";

/// Result of a Firecrawl scrape: markdown content plus whatever structured
/// metadata Firecrawl returned. Metadata fields are individually optional
/// because upstream sets them inconsistently.
#[derive(Debug)]
pub struct ScrapeResult {
    pub markdown: String,
    pub title: Option<String>,
    /// Publication date normalized to `YYYY-MM-DD`, or `None` when Firecrawl
    /// did not return a `publishedTime`.
    pub date_published: Option<String>,
}

fn api_key() -> Result<String> {
    std::env::var("FIRECRAWL_API_KEY").context("FIRECRAWL_API_KEY not set")
}

#[derive(Deserialize)]
struct ScrapeResponse {
    success: bool,
    data: Option<ScrapeData>,
}

#[derive(Deserialize)]
struct ScrapeData {
    markdown: Option<String>,
    metadata: Option<ScrapeMetadata>,
}

/// Subset of Firecrawl's metadata object. Firecrawl returns many more fields
/// (og:image, ogLocaleAlternate, statusCode, ...) but we only consume the
/// ones that feed our deterministic title/date extraction.
#[derive(Deserialize)]
struct ScrapeMetadata {
    title: Option<String>,
    #[serde(rename = "ogTitle")]
    og_title: Option<String>,
    #[serde(rename = "publishedTime")]
    published_time: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

/// Shared HTTP client for Firecrawl API calls — governed in `global-net`.
fn shared_client() -> std::sync::Arc<reqwest::Client> {
    global_net::http::firecrawl_client()
}

/// Scrape a URL using Firecrawl API. Returns markdown content plus any
/// title/publish-date metadata Firecrawl surfaced.
pub async fn scrape(url: &str) -> Result<ScrapeResult> {
    let key = api_key()?;

    let resp = shared_client()
        .post(FIRECRAWL_SCRAPE_URL)
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({
            "url": url,
            "formats": ["markdown"],
        }))
        .send()
        .await
        .with_context(|| format!("firecrawl API request failed for {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("(unable to read error body: {e})"));
        bail!("firecrawl API returned HTTP {status}: {body}");
    }

    let parsed: ScrapeResponse = resp
        .json()
        .await
        .context("firecrawl response parse error")?;

    if !parsed.success {
        bail!("firecrawl API returned success=false for {url}");
    }

    let data = parsed
        .data
        .with_context(|| format!("firecrawl returned no data for {url}"))?;

    let markdown = data
        .markdown
        .with_context(|| format!("firecrawl returned no markdown for {url}"))?;

    let (title, date_published) = data
        .metadata
        .map(|m| {
            let title = m.og_title.or(m.title);
            let raw = m.published_time.as_deref().or(m.modified_time.as_deref());
            let date = raw.and_then(metadata_probe::normalize_date);
            // If Firecrawl gave us a raw date string but normalize rejected it,
            // log so we can detect API schema drift (e.g. field rename) before
            // it silently regresses coverage.
            if let (Some(raw), None) = (raw, date.as_deref()) {
                tracing::warn!(
                    module = "scout-io-firecrawl",
                    url,
                    raw_date = raw,
                    "firecrawl date field present but normalize_date rejected it"
                );
            }
            (title, date)
        })
        .unwrap_or((None, None));

    Ok(ScrapeResult {
        markdown,
        title,
        date_published,
    })
}
