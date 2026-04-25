//! HTTP client for communicating with the mando-gw daemon.

use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use crate::gateway_paths as paths;
use anyhow::{bail, Context, Result};
use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

/// Re-export for use inside the CLI crate. Single source of truth lives in
/// mando-types so the CLI and daemon cannot drift out of sync on where
/// runtime state is read/written.
pub(crate) use global_types::data_dir;

/// Typed error for daemon client failures. Lets the top-level CLI handler
/// render the right "daemon not running" hint without string-matching on
/// formatted error messages.
#[derive(Debug)]
pub(crate) enum DaemonClientError {
    /// No daemon port file was found — daemon was never started.
    NotRunning { port_file: PathBuf },
    /// Port file exists but the TCP connection was refused (daemon crashed
    /// or finished shutdown before the port file was cleaned up).
    ConnectionRefused { source: reqwest::Error },
}

impl fmt::Display for DaemonClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning { port_file } => {
                write!(f, "daemon not running (no {})", port_file.display())
            }
            Self::ConnectionRefused { source } => {
                write!(f, "daemon not running (connection refused): {source}")
            }
        }
    }
}

impl std::error::Error for DaemonClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotRunning { .. } => None,
            Self::ConnectionRefused { source } => Some(source),
        }
    }
}

impl DaemonClientError {
    /// One-line message shown to the user by the top-level CLI handler.
    pub(crate) fn friendly_message(&self) -> String {
        match self {
            Self::NotRunning { .. } => {
                "error: daemon not running. Start with: mando daemon start".to_string()
            }
            Self::ConnectionRefused { .. } => {
                "error: daemon not running (connection refused). Start with: mando daemon start"
                    .to_string()
            }
        }
    }
}

/// Walk the anyhow error chain looking for a typed daemon-client error.
/// Handles the case where a route handler added context on top of the root
/// typed error.
pub(crate) fn find_daemon_error(e: &anyhow::Error) -> Option<&DaemonClientError> {
    e.chain()
        .find_map(|src| src.downcast_ref::<DaemonClientError>())
}

/// Map a reqwest transport error into a typed daemon-client error when the
/// failure was a refused connection; otherwise annotate with `context`.
fn wrap_send_error(e: reqwest::Error, context: &'static str) -> anyhow::Error {
    if e.is_connect() {
        anyhow::Error::new(DaemonClientError::ConnectionRefused { source: e })
    } else {
        anyhow::Error::new(e).context(context)
    }
}

/// Read the prod port file, falling back to the dev port file on `NotFound`.
/// Returns `Ok(None)` when both are absent (daemon never started). Any other
/// I/O error (permission denied, invalid filename, etc.) is propagated with
/// context so the user sees the actionable problem instead of a spurious
/// "daemon not running" hint.
fn read_port_file(
    port_file: &std::path::Path,
    dev_port_file: &std::path::Path,
) -> Result<Option<String>> {
    match std::fs::read_to_string(port_file) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::read_to_string(dev_port_file) {
                Ok(s) => Ok(Some(s)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(anyhow::Error::from(e).context(format!(
                    "failed to read daemon dev port at {}",
                    dev_port_file.display()
                ))),
            }
        }
        Err(e) => Err(anyhow::Error::from(e).context(format!(
            "failed to read daemon port at {}",
            port_file.display()
        ))),
    }
}

/// Discover daemon port and auth token, build a pre-configured client.
pub(crate) struct DaemonClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl DaemonClient {
    /// Create a client by discovering the daemon from ~/.mando/ files.
    pub(crate) fn discover() -> Result<Self> {
        let data_dir = data_dir();

        // Read port: try daemon.port (prod), fall back to daemon-dev.port (dev).
        // Only NotFound on both candidates means "daemon not running" —
        // permission errors or invalid UTF-8 are actionable I/O problems
        // that must surface rather than get masked as NotRunning.
        let port_file = data_dir.join("daemon.port");
        let dev_port_file = data_dir.join("daemon-dev.port");
        let port_str = match read_port_file(&port_file, &dev_port_file)? {
            Some(s) => s,
            None => {
                return Err(DaemonClientError::NotRunning {
                    port_file: port_file.clone(),
                }
                .into())
            }
        };
        let port: u16 = port_str
            .trim()
            .parse()
            .context("invalid port in daemon.port")?;

        // Read auth token.
        let token_file = data_dir.join("auth-token");
        let token = match std::fs::read_to_string(&token_file) {
            Ok(s) => {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(anyhow::Error::from(e).context(format!(
                    "failed to read auth token at {}",
                    token_file.display(),
                )));
            }
        };

        Ok(Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            token,
        })
    }

    /// Send an authenticated JSON request to the daemon and deserialize the response.
    async fn request_json<T, B>(&self, method: Method, path: &str, body: Option<&B>) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.request(method.clone(), &url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| wrap_send_error(e, "daemon request failed"))?;
        let status = resp.status();
        debug!(method = %method, path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon request");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {text}");
        }
        resp.json().await.context("invalid JSON response")
    }

    /// GET request to the daemon, deserialized into a concrete response type.
    pub(crate) async fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request_json::<T, ()>(Method::GET, path, None).await
    }

    /// GET request that returns the raw response body as text (for NDJSON endpoints).
    pub(crate) async fn get_text(&self, path: &str) -> Result<String> {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| wrap_send_error(e, "daemon request failed"))?;
        let status = resp.status();
        debug!(method = "GET", path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon request (text)");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {text}");
        }
        resp.text().await.context("failed to read response body")
    }

    /// GET request that deserializes the JSON body even when the daemon responds with 5xx.
    pub(crate) async fn get_json_with_body_on_5xx<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| wrap_send_error(e, "daemon request failed"))?;
        let status = resp.status();
        debug!(method = "GET", path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon request (allow 5xx)");
        if status.is_success() || status.is_server_error() {
            return resp.json().await.context("invalid JSON response");
        }
        let text = resp.text().await.unwrap_or_default();
        bail!("daemon returned {status}: {text}");
    }

    /// POST request with JSON body, deserialized into a concrete response type.
    pub(crate) async fn post_json<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request_json(Method::POST, path, Some(body)).await
    }

    /// POST for routes that declare `body = api_types::EmptyRequest`.
    /// Sends `{}` with `Content-Type: application/json` so the axum `Json`
    /// extractor on the handler decodes into `EmptyRequest`. Mirrors
    /// `GatewayClient::post_no_body` in `rust/crates/transport-tg/src/http.rs`.
    pub(crate) async fn post_no_body<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.post_json(path, &api_types::EmptyRequest {}).await
    }

    /// PATCH request with JSON body, deserialized into a concrete response type.
    pub(crate) async fn patch_json<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request_json(Method::PATCH, path, Some(body)).await
    }

    /// DELETE request, deserialized into a concrete response type.
    pub(crate) async fn delete_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request_json::<T, ()>(Method::DELETE, path, None).await
    }

    /// POST request with multipart form data, deserialized into a concrete response type.
    pub(crate) async fn post_multipart_json<T>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.post(&url).multipart(form);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| wrap_send_error(e, "daemon request failed"))?;
        let status = resp.status();
        debug!(method = "POST", path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon multipart request");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {text}");
        }
        resp.json().await.context("invalid JSON response")
    }

    /// Health check (no auth needed — uses unauthenticated GET).
    pub(crate) async fn health(&self) -> Result<api_types::HealthResponse> {
        self.health_json().await
    }

    /// Health check (no auth needed), deserialized into a concrete response type.
    pub(crate) async fn health_json<T>(&self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let start = Instant::now();
        let url = format!("{}{}", self.base_url, paths::HEALTH);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| wrap_send_error(e, "daemon not reachable"))?;
        let status = resp.status();
        debug!(method = "GET", path = paths::HEALTH, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon health check");
        resp.json().await.context("invalid JSON response")
    }
}

/// Thin wrapper around `global_types::parse_i64_id` that returns an
/// `anyhow::Error` instead of a plain String, so call sites can use `?`.
pub(crate) fn parse_id(id: &str, label: &str) -> Result<i64> {
    global_types::parse_i64_id(id, label).map_err(|e| anyhow::anyhow!(e))
}
