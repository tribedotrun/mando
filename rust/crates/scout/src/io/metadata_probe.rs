//! Metadata probe — deterministic extraction of title and publish date from
//! raw HTML, tweet URLs, and sundry date formats. Runs before the content
//! reaches readability (which strips `<meta>`, `<time>`, and JSON-LD) or the
//! LLM (which reads prose and frequently guesses wrong).

use std::sync::LazyLock;

use regex::Regex;
use time::format_description::well_known::Iso8601;
use time::{Date, OffsetDateTime};

/// Parsed publication metadata from raw HTML.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HtmlMetadata {
    pub title: Option<String>,
    pub date_published: Option<String>,
}

/// Compile a regex that is a hard-coded program constant. Failure means the
/// source literal itself is malformed, which is a developer error, not a
/// runtime one — treat it as unrecoverable.
fn static_re(name: &'static str, pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => global_infra::unrecoverable!(name, e),
    }
}

// JSON-LD: {"datePublished":"..."} — true publish signal.
static RE_JSONLD_DATE_PUBLISHED: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_JSONLD_DATE_PUBLISHED",
        r#""datePublished"\s*:\s*"([^"]+)""#,
    )
});

// JSON-LD: {"dateCreated":"..."} — fallback. Kept distinct from datePublished
// so a page whose JSON-LD happens to serialize dateCreated before
// datePublished (common with Django REST, Ghost) doesn't surface the earlier
// creation time as the publish date.
static RE_JSONLD_DATE_CREATED: LazyLock<Regex> =
    LazyLock::new(|| static_re("RE_JSONLD_DATE_CREATED", r#""dateCreated"\s*:\s*"([^"]+)""#));

// <meta property="article:published_time" content="..."> — true publish signal.
static RE_META_PROP_PUBLISHED: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_PROP_PUBLISHED",
        r#"(?i)<meta[^>]*property=["'](?:article:published_time|og:article:published_time)["'][^>]*content=["']([^"']+)["']"#,
    )
});

// Reverse attribute order for true-publish signals.
static RE_META_PROP_PUBLISHED_REV: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_PROP_PUBLISHED_REV",
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*property=["'](?:article:published_time|og:article:published_time)["']"#,
    )
});

// <meta property="article:modified_time" content="..."> — last-resort fallback.
// Kept distinct from published_time so a page that was updated in 2024 but
// published in 2020 never presents as 2024-published.
static RE_META_PROP_MODIFIED: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_PROP_MODIFIED",
        r#"(?i)<meta[^>]*property=["'](?:article:modified_time|og:updated_time)["'][^>]*content=["']([^"']+)["']"#,
    )
});

static RE_META_PROP_MODIFIED_REV: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_PROP_MODIFIED_REV",
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*property=["'](?:article:modified_time|og:updated_time)["']"#,
    )
});

// <meta name="date" content="..."> (and misc publishing standards that express
// original publish date, not modification time).
static RE_META_NAME_DATE: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_NAME_DATE",
        r#"(?i)<meta[^>]*name=["'](?:date|pubdate|DC\.date\.issued|sailthru\.date|citation_publication_date|article:published_time)["'][^>]*content=["']([^"']+)["']"#,
    )
});

// <time datetime="...">
static RE_TIME_TAG: LazyLock<Regex> =
    LazyLock::new(|| static_re("RE_TIME_TAG", r#"(?i)<time[^>]*datetime=["']([^"']+)["']"#));

// <meta property="og:title" content="..."> / twitter:title
static RE_META_PROP_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_PROP_TITLE",
        r#"(?i)<meta[^>]*property=["'](?:og:title|twitter:title)["'][^>]*content=["']([^"']+)["']"#,
    )
});

// Some sites use name= instead of property= for OpenGraph.
static RE_META_NAME_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    static_re(
        "RE_META_NAME_TITLE",
        r#"(?i)<meta[^>]*name=["'](?:og:title|twitter:title)["'][^>]*content=["']([^"']+)["']"#,
    )
});

// <title>...</title>
static RE_TITLE_TAG: LazyLock<Regex> =
    LazyLock::new(|| static_re("RE_TITLE_TAG", r#"(?i)<title[^>]*>([^<]+)</title>"#));

// /status/<digits> in a tweet URL.
static RE_TWEET_STATUS_ID: LazyLock<Regex> =
    LazyLock::new(|| static_re("RE_TWEET_STATUS_ID", r#"/status/(\d{5,20})"#));

/// Twitter Snowflake epoch: 2010-11-04 01:42:54.657 UTC.
const TWITTER_EPOCH_MS: i64 = 1_288_834_974_657;

/// Probe raw HTML for a publish title and date. Returns whatever it finds;
/// each field independently falls back to `None` if no reliable signal.
pub fn probe_html(html: &str) -> HtmlMetadata {
    HtmlMetadata {
        title: extract_title(html),
        date_published: extract_date(html),
    }
}

fn extract_title(html: &str) -> Option<String> {
    for re in [&*RE_META_PROP_TITLE, &*RE_META_NAME_TITLE, &*RE_TITLE_TAG] {
        if let Some(raw) = re.captures(html).and_then(|c| c.get(1)).map(|m| m.as_str()) {
            let cleaned = clean_title(raw);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

fn extract_date(html: &str) -> Option<String> {
    // Priority: structured publish signals → unstructured time tag → modified
    // time as last resort. Alternation is not used to merge published and
    // modified (or datePublished and dateCreated): both can coexist in one
    // document, and the wrong one could win on document order.
    for re in [&*RE_JSONLD_DATE_PUBLISHED, &*RE_JSONLD_DATE_CREATED] {
        for caps in re.captures_iter(html) {
            if let Some(m) = caps.get(1) {
                if let Some(norm) = normalize_date(m.as_str()) {
                    return Some(norm);
                }
            }
        }
    }
    for re in [
        &*RE_META_PROP_PUBLISHED,
        &*RE_META_PROP_PUBLISHED_REV,
        &*RE_META_NAME_DATE,
        &*RE_TIME_TAG,
        &*RE_META_PROP_MODIFIED,
        &*RE_META_PROP_MODIFIED_REV,
    ] {
        if let Some(m) = re.captures(html).and_then(|c| c.get(1)) {
            if let Some(norm) = normalize_date(m.as_str()) {
                return Some(norm);
            }
        }
    }
    None
}

fn clean_title(s: &str) -> String {
    let decoded = s
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the tweet status-id from a URL, apply Snowflake math, and return
/// the UTC date as `YYYY-MM-DD`. Returns `None` for non-status URLs or pre-
/// snowflake (pre-Nov 2010) ids.
pub fn snowflake_date_from_tweet_url(url: &str) -> Option<String> {
    let caps = RE_TWEET_STATUS_ID.captures(url)?;
    let id: u64 = caps.get(1)?.as_str().parse().ok()?;
    let offset_ms: i64 = (id >> 22).try_into().ok()?;
    let timestamp_ms = TWITTER_EPOCH_MS.checked_add(offset_ms)?;
    let seconds = timestamp_ms / 1000;
    let odt = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}",
        odt.year(),
        odt.month() as u8,
        odt.day()
    ))
}

/// Normalize a raw date string to `YYYY-MM-DD`. Accepts `YYYYMMDD` (yt-dlp),
/// `YYYY-MM-DD`, and ISO 8601 with time/timezone. Returns `None` for anything
/// else or for calendar-invalid dates.
pub fn normalize_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() == 8 && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return plausible_ymd(&trimmed[0..4], &trimmed[4..6], &trimmed[6..8]);
    }
    // Use `get(0..10)` (not `&trimmed[0..10]`) so a non-ASCII input whose
    // byte-10 lands inside a multibyte sequence (e.g. `2025年10月17日`)
    // returns `None` cleanly instead of panicking at a non-char boundary.
    if let Some(head) = trimmed.get(0..10) {
        let shape_ok = head.chars().enumerate().all(|(i, c)| {
            matches!(i, 4 | 7)
                .then_some(c == '-')
                .unwrap_or(c.is_ascii_digit())
        });
        if shape_ok {
            return plausible_ymd(&head[0..4], &head[5..7], &head[8..10]);
        }
    }
    if let Ok(dt) = OffsetDateTime::parse(trimmed, &Iso8601::DEFAULT) {
        return Some(format!(
            "{:04}-{:02}-{:02}",
            dt.year(),
            dt.month() as u8,
            dt.day()
        ));
    }
    None
}

fn plausible_ymd(y: &str, m: &str, d: &str) -> Option<String> {
    let yy: i32 = y.parse().ok()?;
    let mm: u8 = m.parse().ok()?;
    let dd: u8 = d.parse().ok()?;
    if !(1900..=2100).contains(&yy) {
        return None;
    }
    let month = time::Month::try_from(mm).ok()?;
    Date::from_calendar_date(yy, month, dd).ok()?;
    Some(format!("{yy:04}-{mm:02}-{dd:02}"))
}
