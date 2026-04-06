//! HTTP client for communicating with the mando-gw daemon.

use std::time::Instant;

use anyhow::{bail, Context, Result};
use reqwest::{Client, Method};
use serde_json::Value;
use tracing::debug;

/// Re-export for use inside the CLI crate. Single source of truth lives in
/// mando-types so the CLI and daemon cannot drift out of sync on where
/// runtime state is read/written.
pub(crate) use mando_types::data_dir;

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
        let port_file = data_dir.join("daemon.port");
        let dev_port_file = data_dir.join("daemon-dev.port");
        let port_str = std::fs::read_to_string(&port_file)
            .or_else(|_| std::fs::read_to_string(&dev_port_file))
            .with_context(|| {
                format!(
                    "daemon not running (no {}). Start with: mando daemon start",
                    port_file.display()
                )
            })?;
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

    /// Send an authenticated JSON request to the daemon.
    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.request(method.clone(), &url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.context("daemon request failed")?;
        let status = resp.status();
        debug!(method = %method, path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon request");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {text}");
        }
        resp.json().await.context("invalid JSON response")
    }

    /// GET request to the daemon.
    pub(crate) async fn get(&self, path: &str) -> Result<Value> {
        self.request(Method::GET, path, None).await
    }

    /// GET request that returns the JSON body even when the daemon responds
    /// with 5xx. Used by health endpoints that return HTTP 503 with a
    /// structured body describing the degradation (so `mando health` can
    /// still print the degradation details instead of failing with the
    /// raw status line).
    pub(crate) async fn get_with_body_on_5xx(&self, path: &str) -> Result<Value> {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.context("daemon request failed")?;
        let status = resp.status();
        debug!(method = "GET", path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon request (allow 5xx)");
        if status.is_success() || status.is_server_error() {
            return resp.json().await.context("invalid JSON response");
        }
        let text = resp.text().await.unwrap_or_default();
        bail!("daemon returned {status}: {text}");
    }

    /// POST request with JSON body.
    pub(crate) async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.request(Method::POST, path, Some(body)).await
    }

    /// PATCH request with JSON body.
    pub(crate) async fn patch(&self, path: &str, body: &Value) -> Result<Value> {
        self.request(Method::PATCH, path, Some(body)).await
    }

    /// DELETE request.
    pub(crate) async fn delete(&self, path: &str) -> Result<Value> {
        self.request(Method::DELETE, path, None).await
    }

    /// POST request with multipart form data.
    pub(crate) async fn post_multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value> {
        let start = Instant::now();
        let url = format!("{}{path}", self.base_url);
        let mut req = self.client.post(&url).multipart(form);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.context("daemon request failed")?;
        let status = resp.status();
        debug!(method = "POST", path = path, status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon multipart request");
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {text}");
        }
        resp.json().await.context("invalid JSON response")
    }

    /// Health check (no auth needed — uses unauthenticated GET).
    pub(crate) async fn health(&self) -> Result<Value> {
        let start = Instant::now();
        let url = format!("{}/api/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("daemon not reachable")?;
        let status = resp.status();
        debug!(method = "GET", path = "/api/health", status = %status, elapsed_ms = start.elapsed().as_millis(), "daemon health check");
        resp.json().await.context("invalid JSON response")
    }
}

/// Thin wrapper around `mando_types::parse_i64_id` that returns an
/// `anyhow::Error` instead of a plain String, so call sites can use `?`.
pub(crate) fn parse_id(id: &str, label: &str) -> Result<i64> {
    mando_types::parse_i64_id(id, label).map_err(|e| anyhow::anyhow!(e))
}
