//! Shared generated daemon client with Telegram-specific startup behavior.

use std::ops::Deref;
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// Generated daemon client plus the retry behavior needed while the daemon starts.
#[derive(Clone)]
pub struct GatewayClient(gateway_client::GatewayClient);

impl GatewayClient {
    /// Create with explicit port/token (for testing or when values are known).
    pub fn new(port: u16, token: Option<String>) -> Self {
        Self(gateway_client::GatewayClient::localhost(port, token))
    }

    /// POST the task-add multipart route with Telegram text/file fields.
    pub async fn post_task_add_with_file(
        &self,
        fields: &[(&str, &str)],
        file: Option<(&str, Vec<u8>, &str)>,
    ) -> Result<api_types::TaskItem> {
        let mut form = reqwest::multipart::Form::new();
        for (key, value) in fields {
            form = form.text((*key).to_string(), (*value).to_string());
        }
        if let Some((name, data, filename)) = file {
            let part = reqwest::multipart::Part::bytes(data)
                .file_name(filename.to_string())
                .mime_str("image/jpeg")
                .context("invalid mime")?;
            form = form.part(name.to_string(), part);
        }
        Ok(self.0.post_tasks_add_multipart(form).await?)
    }

    /// Block until the generated health route responds, or timeout expires.
    pub async fn wait_for_gateway(&self, timeout: Duration) -> Result<()> {
        let start = tokio::time::Instant::now();
        let mut delay = Duration::from_millis(100);
        let max_delay = Duration::from_secs(5);

        loop {
            match self.0.get_health().await {
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
}

impl Deref for GatewayClient {
    type Target = gateway_client::GatewayClient;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_preserves_connection_settings() {
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
