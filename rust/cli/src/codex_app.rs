//! `mando codex` — thin client for the daemon-owned Codex desktop-app swap.

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct CodexArgs {
    #[command(subcommand)]
    pub command: CodexCommand,
}

#[derive(Subcommand)]
pub(crate) enum CodexCommand {
    /// Swap a pooled Codex account into the ChatGPT desktop app.
    #[command(name = "app-use")]
    Use {
        /// Pool credential label to swap in.
        label: String,
    },
    /// Sync the checked-out pool credential's rotated tokens back, then
    /// restore the previously stashed personal account.
    #[command(name = "app-restore")]
    Restore,
    /// Show which account currently occupies the ChatGPT desktop app slot.
    #[command(name = "app-status")]
    Status {
        /// Emit machine-readable JSON instead of a human summary.
        #[arg(long)]
        json: bool,
    },
}

pub(crate) async fn handle(args: CodexArgs) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let codex_home = codex_home_override();
    match args.command {
        CodexCommand::Use { label } => {
            let response = client
                .post_credentials_codex_app_use(&api_types::CodexDesktopAppUseRequest {
                    label,
                    codex_home,
                    caller_pid: Some(std::process::id()),
                })
                .await
                .map_err(client_error)?;
            print_operation(response);
            Ok(())
        }
        CodexCommand::Restore => {
            let response = client
                .post_credentials_codex_app_restore(&api_types::CodexDesktopAppRestoreRequest {
                    codex_home,
                })
                .await
                .map_err(client_error)?;
            print_operation(response);
            Ok(())
        }
        CodexCommand::Status { json } => {
            let response = client
                .get_credentials_codex_app_status(&api_types::CodexDesktopAppStatusQuery {
                    codex_home,
                })
                .await
                .map_err(client_error)?;
            print_status(response, json)
        }
    }
}

fn codex_home_override() -> Option<String> {
    std::env::var("CODEX_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn client_error(error: gateway_client::ClientError) -> anyhow::Error {
    if let gateway_client::ClientError::Http { body, .. } = &error {
        if let Ok(response) = serde_json::from_str::<api_types::ErrorResponse>(body) {
            return anyhow::anyhow!(response.error);
        }
    }
    anyhow::Error::new(error)
}

fn print_operation(response: api_types::CodexDesktopAppOperationResponse) {
    for warning in response.warnings {
        eprintln!("{warning}");
    }
    println!("{}", response.message);
}

fn print_status(
    response: api_types::CodexDesktopAppStatusResponse,
    as_json: bool,
) -> anyhow::Result<()> {
    if as_json {
        println!("{}", serde_json::to_string(&response)?);
        return Ok(());
    }

    match response.mode {
        api_types::CodexDesktopAppMode::Pool => {
            let label = response.active_label.as_deref().unwrap_or("?");
            match response.credential_id {
                Some(id) => println!("ChatGPT desktop app: using pool account '{label}' (#{id})"),
                None => println!("ChatGPT desktop app: using pool account '{label}'"),
            }
        }
        api_types::CodexDesktopAppMode::Ambient => {
            println!("ChatGPT desktop app: using personal/ambient account");
        }
        api_types::CodexDesktopAppMode::None => {
            println!("ChatGPT desktop app: unknown (no ~/.codex/auth.json)");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_uses_typed_daemon_message() {
        let error = gateway_client::ClientError::Http {
            status: reqwest::StatusCode::CONFLICT,
            body: r#"{"error":"no stashed personal account to restore"}"#.into(),
        };
        assert_eq!(
            client_error(error).to_string(),
            "no stashed personal account to restore"
        );
    }
}
