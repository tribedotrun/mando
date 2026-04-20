//! HTTP client for communicating with the mando-gw daemon.
//!
//! Mirrors the CLI's `DaemonClient` pattern but adds retry/wait logic
//! needed for the Telegram bot process (which may start before the gateway).

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// HTTP client for gateway REST endpoints.
#[derive(Clone)]
pub struct GatewayClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl GatewayClient {
    /// Discover daemon from `~/.mando/` files (port + auth token).
    pub fn discover() -> Result<Self> {
        let data_dir = global_infra::paths::data_dir();

        let port_file = data_dir.join("daemon.port");
        let port: u16 = std::fs::read_to_string(&port_file)
            .with_context(|| {
                format!(
                    "gateway not running (no {}). Start with: just daemon",
                    port_file.display()
                )
            })?
            .trim()
            .parse()
            .context("invalid port in daemon.port")?;

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

        Ok(Self::new(port, token))
    }

    /// Create with explicit port/token (for testing or when values are known).
    pub fn new(port: u16, token: Option<String>) -> Self {
        Self {
            client: (*global_net::shared_client()).clone(),
            base_url: format!("http://127.0.0.1:{port}"),
            token,
        }
    }

    /// Base URL of the gateway (e.g. `http://127.0.0.1:18791`).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Auth token (if any).
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Underlying HTTP client (for raw requests like image fetching).
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Typed GET: decode response into caller-chosen DTO.
    pub async fn get_typed<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let resp = self
            .authed_request(self.client.get(self.url(path)))
            .send()
            .await
            .context("gateway GET request failed")?;
        Self::check_response_typed(resp).await
    }

    /// Typed POST: serialize `body`, decode response into `R`.
    pub async fn post_typed<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let resp = self
            .authed_request(self.client.post(self.url(path)))
            .json(&serde_json::to_value(body)?)
            .send()
            .await
            .context("gateway POST request failed")?;
        Self::check_response_typed(resp).await
    }

    /// Typed POST for routes that declare `body = api_types::EmptyRequest`.
    /// Sends `{}` with `Content-Type: application/json` so the axum `Json`
    /// extractor on the handler successfully decodes into `EmptyRequest`.
    pub async fn post_no_body<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        self.post_typed::<api_types::EmptyRequest, R>(path, &api_types::EmptyRequest {})
            .await
    }

    /// Typed PATCH: serialize `body`, decode response into `R`.
    pub async fn patch_typed<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let resp = self
            .authed_request(self.client.patch(self.url(path)))
            .json(&serde_json::to_value(body)?)
            .send()
            .await
            .context("gateway PATCH request failed")?;
        Self::check_response_typed(resp).await
    }

    /// Typed DELETE: decode response into `R`.
    pub async fn delete_typed<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let resp = self
            .authed_request(self.client.delete(self.url(path)))
            .send()
            .await
            .context("gateway DELETE request failed")?;
        Self::check_response_typed(resp).await
    }

    /// GET that returns the JSON body even when the gateway responds with
    /// 5xx, decoded into `R`. Used by `/api/health/system`, which returns
    /// HTTP 503 with a structured body when the daemon is degraded so the
    /// bot's `/health` command can still print the degradation details.
    pub async fn get_with_body_on_5xx_typed<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let resp = self
            .authed_request(self.client.get(self.url(path)))
            .send()
            .await
            .context("gateway GET request failed")?;
        let status = resp.status();
        if status.is_success() || status.is_server_error() {
            return resp
                .json()
                .await
                .context("failed to parse gateway JSON response");
        }
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway returned {status}: {text}")
    }

    /// Health check (no auth required).
    pub async fn health(&self) -> Result<api_types::HealthResponse> {
        let resp = self
            .client
            .get(self.url("/api/health"))
            .send()
            .await
            .context("gateway not reachable")?;
        resp.json()
            .await
            .context("invalid JSON from health endpoint")
    }

    /// POST multipart form with text fields + optional binary file, decoded
    /// into a concrete response DTO.
    pub async fn post_multipart_with_file<R: DeserializeOwned>(
        &self,
        path: &str,
        fields: &[(&str, &str)],
        file: Option<(&str, Vec<u8>, &str)>, // (field_name, bytes, filename)
    ) -> Result<R> {
        let mut form = reqwest::multipart::Form::new();
        for (k, v) in fields {
            form = form.text(k.to_string(), v.to_string());
        }
        if let Some((name, data, filename)) = file {
            let part = reqwest::multipart::Part::bytes(data)
                .file_name(filename.to_string())
                .mime_str("image/jpeg")
                .context("invalid mime")?;
            form = form.part(name.to_string(), part);
        }
        let resp = self
            .authed_request(self.client.post(self.url(path)))
            .multipart(form)
            .send()
            .await
            .context("gateway POST multipart request failed")?;
        Self::check_response_typed(resp).await
    }

    /// Block until gateway responds to `/api/health`, or timeout expires.
    ///
    /// Uses exponential backoff: 100ms → 200ms → 400ms → … capped at 5s.
    pub async fn wait_for_gateway(&self, timeout: Duration) -> Result<()> {
        let start = tokio::time::Instant::now();
        let mut delay = Duration::from_millis(100);
        let max_delay = Duration::from_secs(5);

        loop {
            match self.health().await {
                Ok(_) => return Ok(()),
                Err(_) if start.elapsed() >= timeout => {
                    bail!(
                        "gateway did not become available within {}s",
                        timeout.as_secs()
                    );
                }
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }

    // ── helpers ──────────────────────────────────────────────────────

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn authed_request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    async fn check_response_typed<R: DeserializeOwned>(resp: reqwest::Response) -> Result<R> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("gateway returned {status}: {body}");
        }
        resp.json().await.context("invalid JSON response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_builds_correct_url() {
        let client = GatewayClient::new(18791, Some("test-token".into()));
        assert_eq!(client.base_url(), "http://127.0.0.1:18791");
        assert_eq!(client.token(), Some("test-token"));
    }

    #[test]
    fn new_without_token() {
        let client = GatewayClient::new(9999, None);
        assert_eq!(client.base_url(), "http://127.0.0.1:9999");
        assert_eq!(client.token(), None);
    }
}
