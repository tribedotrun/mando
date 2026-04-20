//! Firecrawl integration — web scraping via Firecrawl v1 API.
//!
//! API key read from `FIRECRAWL_API_KEY` env var (injected via config.json env section).

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const FIRECRAWL_SCRAPE_URL: &str = "https://api.firecrawl.dev/v1/scrape";

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
}

/// Shared HTTP client for Firecrawl API calls — governed in `global-net`.
fn shared_client() -> std::sync::Arc<reqwest::Client> {
    global_net::http::firecrawl_client()
}

/// Scrape a URL using Firecrawl API. Returns markdown content.
pub async fn scrape(url: &str) -> Result<String> {
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

    parsed
        .data
        .and_then(|d| d.markdown)
        .with_context(|| format!("firecrawl returned no markdown for {url}"))
}
