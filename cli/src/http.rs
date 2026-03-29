//! HTTP client for communicating with the mando-gw daemon.

use std::time::Instant;

use anyhow::{bail, Context, Result};
use reqwest::{Client, Method};
use serde_json::Value;
use tracing::debug;

/// Resolve the mando data directory (~/.mando or MANDO_DATA_DIR).
/// Must stay in sync with mando_types::data_dir().
pub(crate) fn data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("MANDO_DATA_DIR") {
        if let Some(rest) = dir.strip_prefix("~/") {
            return home_dir().join(rest);
        }
        return std::path::PathBuf::from(dir);
    }
    home_dir().join(".mando")
}

fn home_dir() -> std::path::PathBuf {
    dirs::home_dir().expect("could not determine home directory — set MANDO_DATA_DIR or HOME")
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
        let token = std::fs::read_to_string(&token_file)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

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
        let elapsed_ms = start.elapsed().as_millis();
        debug!(method = %method, path = path, status = %status, elapsed_ms = elapsed_ms, "daemon request");
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
        let mut req = self
            .client
            .post(format!("{}{path}", self.base_url))
            .multipart(form);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.context("daemon request failed")?;
        let status = resp.status();
        let elapsed_ms = start.elapsed().as_millis();
        debug!(method = "POST", path = path, status = %status, elapsed_ms = elapsed_ms, "daemon multipart request");
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("daemon returned {status}: {body}");
        }
        resp.json().await.context("invalid JSON response")
    }

    /// Health check (no auth needed).
    pub(crate) async fn health(&self) -> Result<Value> {
        let start = Instant::now();
        let resp = self
            .client
            .get(format!("{}/api/health", self.base_url))
            .send()
            .await
            .context("daemon not reachable")?;
        let status = resp.status();
        let elapsed_ms = start.elapsed().as_millis();
        debug!(method = "GET", path = "/api/health", status = %status, elapsed_ms = elapsed_ms, "daemon health check");
        resp.json().await.context("invalid JSON response")
    }
}
