//! CLI-local daemon discovery and error presentation.

use std::path::PathBuf;

use anyhow::{Context, Result};

pub(crate) use global_types::data_dir;

#[derive(Debug, thiserror::Error)]
#[error("daemon not running (no {port_file})", port_file = .port_file.display())]
pub(crate) struct DaemonNotRunning {
    port_file: PathBuf,
}

/// Return the CLI-specific hint for a daemon discovery or connection failure.
pub(crate) fn daemon_friendly_message(error: &anyhow::Error) -> Option<&'static str> {
    for source in error.chain() {
        if source.downcast_ref::<DaemonNotRunning>().is_some() {
            return Some("error: daemon not running. Start with: mando daemon start");
        }
        if source
            .downcast_ref::<gateway_client::ClientError>()
            .is_some_and(gateway_client::ClientError::is_connect)
        {
            return Some(
                "error: daemon not running (connection refused). Start with: mando daemon start",
            );
        }
    }
    None
}

fn read_port_file(
    port_file: &std::path::Path,
    dev_port_file: &std::path::Path,
) -> Result<Option<String>> {
    match std::fs::read_to_string(port_file) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::read_to_string(dev_port_file) {
                Ok(value) => Ok(Some(value)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(anyhow::Error::from(error).context(format!(
                    "failed to read daemon dev port at {}",
                    dev_port_file.display()
                ))),
            }
        }
        Err(error) => Err(anyhow::Error::from(error).context(format!(
            "failed to read daemon port at {}",
            port_file.display()
        ))),
    }
}

pub(crate) struct DaemonClient(gateway_client::GatewayClient);

impl DaemonClient {
    pub(crate) fn discover() -> Result<Self> {
        let data_dir = data_dir();
        let port_file = data_dir.join("daemon.port");
        let dev_port_file = data_dir.join("daemon-dev.port");
        let port_text =
            read_port_file(&port_file, &dev_port_file)?.ok_or(DaemonNotRunning { port_file })?;
        let port: u16 = port_text
            .trim()
            .parse()
            .context("invalid port in daemon.port")?;

        let token_file = data_dir.join("auth-token");
        let token = match std::fs::read_to_string(&token_file) {
            Ok(value) => {
                let trimmed = value.trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(anyhow::Error::from(error).context(format!(
                    "failed to read auth token at {}",
                    token_file.display()
                )))
            }
        };

        Ok(Self(gateway_client::GatewayClient::with_client(
            format!("http://127.0.0.1:{port}"),
            token,
            reqwest::Client::new(),
        )))
    }

    pub(crate) fn accepting_server_error_bodies(&self) -> gateway_client::GatewayClient {
        self.0.accepting_server_error_bodies()
    }
}

impl std::ops::Deref for DaemonClient {
    type Target = gateway_client::GatewayClient;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) fn parse_id(id: &str, label: &str) -> Result<i64> {
    global_types::parse_i64_id(id, label).map_err(|error| anyhow::anyhow!(error))
}
