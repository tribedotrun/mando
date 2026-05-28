//! Shared HTTP clients. Every outbound HTTP request in Mando builds off one
//! of the clients in this module so that connection pooling, timeouts, and
//! user-agent defaults stay consistent and are governed in one place.
//!
//! The C5 guardrail (`check_runtime_hygiene.py`) bans direct
//! `reqwest::Client`/`ClientBuilder` construction outside `global-net`;
//! callers needing a specialized client (custom timeout, custom UA,
//! custom redirect policy) must add a named helper here rather than
//! inlining a `reqwest::ClientBuilder` call.
//!
//! The general-purpose `shared_client()` is the default choice. The named
//! helpers below cover specialized cases whose settings must stay
//! different (probes that must present a specific user-agent to upstream
//! beta APIs, scrapers that need a browser-like UA, etc.).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Default request timeout. Long enough for streaming reads and Telegraph
/// uploads; short enough that a stuck connection cannot wedge a captain
/// tick or a Telegram long-poll indefinitely.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Default connect timeout. Separate from the overall request timeout so
/// that a captive-portal-style hang fails fast.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// User-Agent used by all outbound HTTP that doesn't require a specific
/// upstream-facing identity. Identifies Mando in upstream access logs
/// without pinning a specific version (we don't want the UA to become a
/// cache-busting key).
const USER_AGENT: &str = "mando/1.0";

/// Browser-like User-Agent for content scraping that must bypass generic
/// bot-filtering. Kept as a single canonical value so it is easy to bump.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

/// User-Agent for Anthropic credential usage probe. The OAuth-beta-gated
/// `/v1/messages` endpoint rejects any UA it does not recognize as Claude
/// Code, so this must look like the real CLI. The exact version does not
/// need to match the user's installed binary.
const PROBE_USER_AGENT: &str = "claude-code/2.1.0";

static SHARED: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static SSE: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static HTML_FETCH: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static HTML_FETCH_NO_REDIRECT: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static FIRECRAWL: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static TELEGRAPH: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static YT_DLP: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
static USAGE_PROBE: OnceLock<Arc<reqwest::Client>> = OnceLock::new();

fn cached<F>(cell: &'static OnceLock<Arc<reqwest::Client>>, build: F) -> Arc<reqwest::Client>
where
    F: FnOnce() -> reqwest::Client,
{
    cell.get_or_init(|| Arc::new(build())).clone()
}

/// Returns the process-wide shared HTTP client. Safe to call from any
/// context; the underlying `reqwest::Client` is internally `Arc`-wrapped
/// and cheap to clone.
pub fn shared_client() -> Arc<reqwest::Client> {
    cached(&SHARED, || {
        reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|e| {
                global_infra::unrecoverable!("shared reqwest client build failed", e)
            })
    })
}

/// HTTP client for long-lived SSE streams. No overall request timeout
/// (the connection stays open as long as the upstream emits events) but
/// keeps the shared connect timeout so a stuck handshake fails fast.
pub fn sse_client() -> Arc<reqwest::Client> {
    cached(&SSE, || {
        reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("sse_client build failed", e))
    })
}

/// HTML content-fetch client for scout's readability pipeline. 30s timeout,
/// browser-like UA, follows up to 10 redirects.
pub fn html_fetch_client() -> Arc<reqwest::Client> {
    cached(&HTML_FETCH, || {
        reqwest::Client::builder()
            .user_agent(BROWSER_USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("html_fetch_client build failed", e))
    })
}

/// HTML client that does not follow redirects — used for URL resolution
/// (e.g. unwrapping `t.co` short links).
pub fn html_fetch_no_redirect_client() -> Arc<reqwest::Client> {
    cached(&HTML_FETCH_NO_REDIRECT, || {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                global_infra::unrecoverable!("html_fetch_no_redirect_client build failed", e)
            })
    })
}

/// Client for Firecrawl (JS-rendered page rescue). 45s timeout.
pub fn firecrawl_client() -> Arc<reqwest::Client> {
    cached(&FIRECRAWL, || {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("firecrawl_client build failed", e))
    })
}

/// Client for Telegraph publishing. 15s timeout.
pub fn telegraph_client() -> Arc<reqwest::Client> {
    cached(&TELEGRAPH, || {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("telegraph_client build failed", e))
    })
}

/// Client used by scout's yt-dlp fallback when probing video metadata.
/// Short connect timeout; longer overall timeout since yt-dlp probes can
/// legitimately take a couple of minutes for slow upstream hosts.
pub fn yt_dlp_client() -> Arc<reqwest::Client> {
    cached(&YT_DLP, || {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("yt_dlp_client build failed", e))
    })
}

/// Client for the credential rate-limit usage probe. Requires a
/// Claude-Code-looking UA to pass the OAuth-beta gate. 15s timeout.
pub fn usage_probe_client() -> Arc<reqwest::Client> {
    cached(&USAGE_PROBE, || {
        reqwest::Client::builder()
            .user_agent(PROBE_USER_AGENT)
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|e| global_infra::unrecoverable!("usage_probe_client build failed", e))
    })
}
