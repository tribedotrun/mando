//! Telegraph (telegra.ph) publishing — create pages for clean article reading.
//!
//! Token cached at `~/.mando/scout/telegraph_token.txt`.
//! Page URLs cached at `content/{id:03d}-telegraph.json`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::file_store;

const API_BASE: &str = "https://api.telegra.ph";

fn token_path() -> PathBuf {
    file_store::scout_dir().join("telegraph_token.txt")
}

fn cache_path(id: i64) -> PathBuf {
    file_store::telegraph_cache_path(id)
}

/// Return cached Telegraph URL for an item, or None.
pub fn get_cached_url(id: i64) -> Option<String> {
    let path = cache_path(id);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return None,
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(data) => data["url"].as_str().map(|s| s.to_string()),
        Err(e) => {
            tracing::warn!(id, %e, "telegraph: corrupt cache file, ignoring");
            None
        }
    }
}

/// Publish article to Telegraph. Returns URL. Idempotent (returns cache if exists).
pub async fn publish_article(id: i64, title: &str, article_md: &str) -> Result<String> {
    if let Some(cached) = get_cached_url(id) {
        return Ok(cached);
    }

    let token = ensure_token().await?;
    let html = markdown_to_telegraph_html(article_md);
    let result = create_page(&token, title, &html).await?;

    let url = result["url"]
        .as_str()
        .context("Telegraph response missing url")?
        .to_string();

    // Cache the result
    let cache = cache_path(id);
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("telegraph: create cache dir {}", parent.display()))?;
    }
    std::fs::write(&cache, serde_json::to_string(&result)?)
        .with_context(|| format!("telegraph: write cache for #{id}"))?;

    tracing::info!(id, %url, "telegraph: published");
    Ok(url)
}

async fn ensure_token() -> Result<String> {
    let path = token_path();
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    tracing::info!("telegraph: creating new account");
    let resp: Value = shared_client()
        .post(format!("{API_BASE}/createAccount"))
        .json(&json!({"short_name": "Mando Scout"}))
        .send()
        .await?
        .json()
        .await?;

    if resp["ok"].as_bool() != Some(true) {
        anyhow::bail!("Telegraph createAccount failed: {}", resp);
    }

    let token = resp["result"]["access_token"]
        .as_str()
        .context("Telegraph createAccount missing access_token")?
        .to_string();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("telegraph: create token dir {}", parent.display()))?;
    }
    std::fs::write(&path, &token)?;
    Ok(token)
}

fn shared_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client")
    })
}

async fn create_page(token: &str, title: &str, html: &str) -> Result<Value> {
    let nodes = html_to_telegraph_nodes(html);
    let nodes_json = serde_json::to_string(&nodes)?;

    let resp: Value = shared_client()
        .post(format!("{API_BASE}/createPage"))
        .form(&[
            ("access_token", token),
            ("title", title),
            ("content", &nodes_json),
            ("author_name", "Mando Scout"),
        ])
        .send()
        .await?
        .json()
        .await?;

    if resp["ok"].as_bool() != Some(true) {
        anyhow::bail!("Telegraph createPage failed: {}", resp);
    }
    Ok(resp["result"].clone())
}

/// Convert simple HTML to Telegraph Node JSON array.
/// Single-pass: splits on closing block tags, determines tag type from opening tag.
fn html_to_telegraph_nodes(html: &str) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut remaining = html;

    while !remaining.is_empty() {
        // Find the next closing block tag.
        let closes = ["</p>", "</h3>", "</h4>", "</blockquote>", "</li>"];
        let next = closes
            .iter()
            .filter_map(|tag| remaining.find(tag).map(|pos| (pos, *tag)))
            .min_by_key(|(pos, _)| *pos);

        let (segment, rest) = match next {
            Some((pos, tag)) => {
                let end = pos + tag.len();
                (&remaining[..pos], &remaining[end..])
            }
            None => (remaining, ""),
        };

        remaining = rest;

        // Determine the Telegraph tag from the opening tag.
        let tg_tag = if segment.contains("<h3>") {
            "h3"
        } else if segment.contains("<h4>") {
            "h4"
        } else if segment.contains("<blockquote>") {
            "blockquote"
        } else {
            "p"
        };

        // Strip all HTML tags to get plain text.
        let text = strip_html_tags(segment);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            nodes.push(json!({"tag": tg_tag, "children": [trimmed]}));
        }
    }

    if nodes.is_empty() {
        nodes.push(json!({"tag": "p", "children": [strip_html_tags(html).trim()]}));
    }
    nodes
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}

/// Convert markdown to Telegraph-compatible HTML.
///
/// Telegraph supports: p, h3, h4, a, b, strong, em, code, pre, blockquote,
/// ul, ol, li, br, hr. Does NOT support h1/h2 — mapped to h3/h4.
fn markdown_to_telegraph_html(md: &str) -> String {
    if md.is_empty() {
        return "<p></p>".into();
    }

    // Split into blocks on blank lines
    let blocks: Vec<&str> = md.split("\n\n").collect();
    let mut html = String::new();

    for block in &blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        // Headings
        if let Some(rest) = block.strip_prefix("### ") {
            html.push_str(&format!("<h4>{}</h4>", inline_fmt(rest)));
        } else if let Some(rest) = block.strip_prefix("## ") {
            html.push_str(&format!("<h4>{}</h4>", inline_fmt(rest)));
        } else if let Some(rest) = block.strip_prefix("# ") {
            html.push_str(&format!("<h3>{}</h3>", inline_fmt(rest)));
        }
        // Blockquote
        else if block.starts_with('>') {
            let lines: Vec<&str> = block
                .lines()
                .map(|l| {
                    l.strip_prefix("> ")
                        .or_else(|| l.strip_prefix(">"))
                        .unwrap_or(l)
                })
                .collect();
            html.push_str(&format!(
                "<blockquote>{}</blockquote>",
                inline_fmt(&lines.join(" "))
            ));
        }
        // Unordered list
        else if block.starts_with("- ") || block.starts_with("* ") {
            let items: Vec<&str> = block
                .lines()
                .map(|l| {
                    l.strip_prefix("- ")
                        .or_else(|| l.strip_prefix("* "))
                        .unwrap_or(l)
                })
                .collect();
            html.push_str("<ul>");
            for item in &items {
                if !item.is_empty() {
                    html.push_str(&format!("<li>{}</li>", inline_fmt(item)));
                }
            }
            html.push_str("</ul>");
        }
        // HR
        else if block.len() >= 3 && block.chars().all(|c| c == '-' || c == '*' || c == '_') {
            html.push_str("<hr/>");
        }
        // Paragraph
        else {
            html.push_str(&format!("<p>{}</p>", inline_fmt(block)));
        }
    }
    html
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn inline_fmt(text: &str) -> String {
    let mut s = escape(text);
    // Bold
    while let Some(start) = s.find("**") {
        if let Some(end) = s[start + 2..].find("**") {
            let end = start + 2 + end;
            let inner = s[start + 2..end].to_string();
            s = format!("{}<strong>{inner}</strong>{}", &s[..start], &s[end + 2..]);
        } else {
            break;
        }
    }
    // Single newlines → <br/>
    s = s.replace('\n', "<br/>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_conversion() {
        let html = markdown_to_telegraph_html("# Title\n\n## Sub\n\nText");
        assert!(html.contains("<h3>Title</h3>"));
        assert!(html.contains("<h4>Sub</h4>"));
        assert!(html.contains("<p>Text</p>"));
    }

    #[test]
    fn bold_conversion() {
        let html = markdown_to_telegraph_html("Hello **world**");
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn list_conversion() {
        let html = markdown_to_telegraph_html("- one\n- two");
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<li>two</li>"));
    }

    #[test]
    fn blockquote_conversion() {
        let html = markdown_to_telegraph_html("> quoted text");
        assert!(html.contains("<blockquote>quoted text</blockquote>"));
    }
}
