//! CLI subcommand: credential pool inspection and pick-for-shell — pure HTTP client.
//!
//! `pick` is the shell-wrapper integration point (iTerm2, `cc`, `cx`):
//!
//! ```sh
//! claude() {
//!   eval "$(command mando credentials pick 2>/dev/null)" || true
//!   command claude "$@"
//! }
//! ```
//!
//! Codex users enter through `cx` or `mdo create --codex`. Those launchers call
//! `codex-pooled-launch.sh`, which uses `pick --codex` as internal plumbing to
//! materialize a per-process `CODEX_HOME` (picked `auth.json` + symlinks to
//! `~/.codex` session state), run `codex`, then `sync-codex`. ChatGPT OAuth
//! tokens are JWT-shaped and must not be passed through `CODEX_ACCESS_TOKEN`.

use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct CredentialsArgs {
    #[command(subcommand)]
    pub command: CredentialsCommand,
}

#[derive(Subcommand)]
pub(crate) enum CredentialsCommand {
    /// List stored credentials (masked tokens, current rate-limit/cooldown).
    #[command(visible_alias = "ls")]
    List,
    /// Disable a credential so automatic and explicit picks cannot select it.
    Disable {
        /// Credential database id.
        id: i64,
    },
    /// Re-enable a disabled credential.
    Enable {
        /// Credential database id.
        id: i64,
    },
    /// Pick the best-available credential right now and emit shell exports
    /// (success) or unsets (any fallback path) so `eval "$(mando credentials pick)"`
    /// always leaves the shell in a correct state.
    Pick {
        /// Pick a Codex OAuth credential instead of Claude. Internal plumbing for Codex launchers.
        #[arg(long)]
        codex: bool,
        /// Pick this credential by database id (overrides auto-pick).
        #[arg(long, conflicts_with_all = ["label", "account"])]
        id: Option<i64>,
        /// Pick this credential by label (overrides auto-pick).
        #[arg(long, conflicts_with_all = ["id", "account"])]
        label: Option<String>,
        /// Shorthand for `--id` when numeric, otherwise `--label`.
        #[arg(long, short = 'a', conflicts_with_all = ["id", "label"])]
        account: Option<String>,
    },
    /// Persist refreshed tokens from `$CODEX_HOME/auth.json` back to the daemon.
    /// Called by `codex-pooled-launch.sh` after a Codex session ends.
    SyncCodex,
}

pub(crate) async fn handle(args: CredentialsArgs) -> Result<()> {
    match args.command {
        CredentialsCommand::List => handle_list().await,
        CredentialsCommand::Disable { id } => handle_set_disabled(id, true).await,
        CredentialsCommand::Enable { id } => handle_set_disabled(id, false).await,
        CredentialsCommand::Pick {
            codex,
            id,
            label,
            account,
        } => {
            let request = build_pick_request(id, label, account)?;
            if codex {
                handle_pick_codex(request).await
            } else {
                handle_pick_claude(request).await
            }
        }
        CredentialsCommand::SyncCodex => handle_sync_codex().await,
    }
}

fn build_pick_request(
    id: Option<i64>,
    label: Option<String>,
    account: Option<String>,
) -> Result<api_types::CredentialPickRequest> {
    if let Some(account) = account {
        let account = account.trim();
        if account.is_empty() {
            bail!("account must not be empty");
        }
        if account.chars().all(|c| c.is_ascii_digit()) {
            let id = account
                .parse::<i64>()
                .map_err(|_| anyhow::anyhow!("invalid account id {account:?}"))?;
            return Ok(api_types::CredentialPickRequest {
                id: Some(id),
                label: None,
            });
        }
        return Ok(api_types::CredentialPickRequest {
            id: None,
            label: Some(account.to_string()),
        });
    }

    let label = match label {
        None => None,
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                bail!("label must not be empty");
            }
            Some(trimmed.to_string())
        }
    };
    Ok(api_types::CredentialPickRequest { id, label })
}

async fn handle_list() -> Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get_credentials().await?;

    if result.credentials.is_empty() {
        println!("No credentials configured.");
        println!();
        println!("Run `claude setup-token` to obtain an OAuth token, then add it via");
        println!("the Mando UI Settings > Accounts page (or POST /api/credentials/setup-token).");
        return Ok(());
    }

    println!(
        "{:<4} {:<10} {:<24} {:<14} {:>6}  TOKEN",
        "ID", "PROVIDER", "LABEL", "STATE", "5H%"
    );
    for cred in &result.credentials {
        let state = if cred.is_disabled {
            "disabled"
        } else if cred.is_expired {
            "expired"
        } else if cred.is_rate_limited {
            "rate-limited"
        } else {
            "ok"
        };
        let util = cred
            .five_hour
            .as_ref()
            .map(|w| format!("{:>5.1}", w.utilization * 100.0))
            .unwrap_or_else(|| "  -  ".into());
        let provider = match cred.provider {
            api_types::CredentialProvider::Codex => "codex",
            api_types::CredentialProvider::Claude => "claude",
        };
        println!(
            "{:<4} {:<10} {:<24} {:<14} {:>6}  {}",
            cred.id, provider, cred.label, state, util, cred.token_masked
        );
    }
    Ok(())
}

async fn handle_set_disabled(id: i64, disabled: bool) -> Result<()> {
    let client = DaemonClient::discover()?;
    let params = api_types::CredentialIdParams { id };
    if disabled {
        client.post_credentials_by_id_disable(&params).await?;
    } else {
        client.post_credentials_by_id_enable(&params).await?;
    }
    if disabled {
        println!("Credential {id} disabled.");
    } else {
        println!("Credential {id} enabled.");
    }
    Ok(())
}

async fn handle_pick_claude(request: api_types::CredentialPickRequest) -> Result<()> {
    let explicit = request.id.is_some() || request.label.is_some();
    let Ok(client) = DaemonClient::discover() else {
        emit_claude_unset();
        return Ok(());
    };

    let result = match client.post_credentials_pick(&request).await {
        Ok(r) => r,
        Err(_) => {
            emit_claude_unset();
            return Ok(());
        }
    };

    if let Some(pick) = result.pick {
        let token = shell_single_quote(&pick.token);
        let label = shell_single_quote(&pick.label);
        println!("export CLAUDE_CODE_OAUTH_TOKEN={token}");
        println!("export MANDO_CREDENTIAL_LABEL={label}");
        println!("export MANDO_CREDENTIAL_ID={}", pick.id);
        eprintln!("mando: using credential '{}' (#{})", pick.label, pick.id);
    } else {
        emit_claude_unset();
        if explicit {
            eprintln!("mando: requested Claude credential not found or wrong provider; falling through to ambient login");
        } else {
            eprintln!(
                "mando: no credentials available (none configured, all expired, or all rate-limited); falling through to ambient login"
            );
        }
    }
    Ok(())
}

async fn handle_pick_codex(request: api_types::CredentialPickRequest) -> Result<()> {
    let explicit = request.id.is_some() || request.label.is_some();
    let client = match DaemonClient::discover() {
        Ok(client) => client,
        Err(err) => {
            emit_codex_unset();
            eprintln!(
                "mando: codex credential pick failed: {err}; falling back to ambient ~/.codex login"
            );
            return Ok(());
        }
    };

    let pick_result = client.post_credentials_codex_pick(&request).await;
    let result = match pick_result {
        Ok(r) => r,
        Err(err) => {
            emit_codex_unset();
            eprintln!(
                "mando: codex credential pick failed: {err}; falling back to ambient ~/.codex login"
            );
            return Ok(());
        }
    };

    if let Some(pick) = result.pick {
        match crate::credentials_codex_pick::materialize_codex_home(&pick.auth_json).await {
            Ok(codex_home) => {
                let account_id = shell_single_quote(&pick.account_id);
                let label = shell_single_quote(&pick.label);
                let home = shell_single_quote(&codex_home.to_string_lossy());
                println!("unset CODEX_ACCESS_TOKEN");
                println!("export CODEX_HOME={home}");
                println!("export MANDO_CODEX_ACCOUNT_ID={account_id}");
                println!("export MANDO_CODEX_CREDENTIAL_LABEL={label}");
                println!("export MANDO_CODEX_CREDENTIAL_ID={}", pick.id);
                println!("export MANDO_CODEX_HOME_MANAGED=1");
                eprintln!(
                    "mando: using Codex credential '{}' (#{}, account {})",
                    pick.label, pick.id, pick.account_id
                );
            }
            Err(err) => {
                emit_codex_unset();
                eprintln!(
                    "mando: codex credential pick failed: {err}; falling back to ambient ~/.codex login"
                );
            }
        }
    } else {
        emit_codex_unset();
        if explicit {
            eprintln!("mando: requested Codex credential not found or unusable; falling through to ambient login");
        } else {
            eprintln!(
                "mando: no Codex credentials available (none configured, all expired, or all rate-limited); falling through to ambient login"
            );
        }
    }
    Ok(())
}

fn emit_claude_unset() {
    println!("unset CLAUDE_CODE_OAUTH_TOKEN");
    println!("unset MANDO_CREDENTIAL_LABEL");
    println!("unset MANDO_CREDENTIAL_ID");
}

fn emit_codex_unset() {
    println!("unset CODEX_ACCESS_TOKEN");
    if should_clear_codex_home_on_fallback() {
        println!("unset CODEX_HOME");
    }
    println!("unset MANDO_CODEX_ACCOUNT_ID");
    println!("unset MANDO_CODEX_CREDENTIAL_LABEL");
    println!("unset MANDO_CODEX_CREDENTIAL_ID");
    println!("unset MANDO_CODEX_HOME_MANAGED");
}

fn should_clear_codex_home_on_fallback() -> bool {
    if std::env::var("MANDO_CODEX_HOME_MANAGED").ok().as_deref() == Some("1") {
        return true;
    }
    std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .is_some_and(|path| crate::credentials_codex_pick::is_managed_codex_home(&path))
}

async fn handle_sync_codex() -> Result<()> {
    let credential_id = match std::env::var("MANDO_CODEX_CREDENTIAL_ID") {
        Ok(raw) => match raw.trim().parse::<i64>() {
            Ok(id) if id > 0 => id,
            _ => return Ok(()),
        },
        Err(_) => return Ok(()),
    };

    let auth_path = crate::credentials_codex_pick::codex_home_auth_json_path()?;
    let auth_json = match std::fs::read_to_string(&auth_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    let Ok(client) = DaemonClient::discover() else {
        return Ok(());
    };

    let body = api_types::SyncCodexCredentialRequest {
        credential_id,
        auth_json,
    };
    let sync_result = client.post_credentials_codex_sync(&body).await;
    if let Err(err) = sync_result {
        // The daemon rejected the sync (e.g. the refresh token rotated to a
        // value it no longer recognizes). CODEX_HOME is deliberately left in
        // place below (we return before the cleanup call) so the rotated
        // tokens on disk stay recoverable — surface both facts.
        match std::env::var("CODEX_HOME") {
            Ok(home) => eprintln!(
                "mando: codex credential sync failed: {err}; temp CODEX_HOME retained at {home} (rotated tokens may still be recoverable there)"
            ),
            Err(_) => eprintln!("mando: codex credential sync failed: {err}"),
        }
        return Err(err.into());
    }

    if let Ok(home) = std::env::var("CODEX_HOME") {
        let path = std::path::PathBuf::from(home);
        if crate::credentials_codex_pick::is_managed_codex_home(&path) {
            crate::credentials_codex_pick::cleanup_managed_codex_home(&path).await?;
        }
    }
    eprintln!("mando: synced Codex credential #{credential_id}");
    Ok(())
}

/// Single-quote a string for safe inclusion in a shell `export ...` line.
/// POSIX rule: inside `'...'` everything is literal except `'`, which we
/// close, escape with `'\''`, and reopen.
fn shell_single_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_single_quote_plain_token() {
        assert_eq!(shell_single_quote("sk-ant-abc"), "'sk-ant-abc'");
    }

    #[test]
    fn shell_single_quote_embedded_single_quote() {
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn shell_single_quote_empty() {
        assert_eq!(shell_single_quote(""), "''");
    }

    #[test]
    fn build_pick_request_account_numeric_is_id() {
        let req = build_pick_request(None, None, Some("3".into())).unwrap();
        assert_eq!(req.id, Some(3));
        assert!(req.label.is_none());
    }

    #[test]
    fn build_pick_request_account_label() {
        let req = build_pick_request(None, None, Some("b_gmail".into())).unwrap();
        assert!(req.id.is_none());
        assert_eq!(req.label.as_deref(), Some("b_gmail"));
    }

    #[test]
    fn build_pick_request_explicit_id_and_label() {
        let req = build_pick_request(Some(7), Some("Portugal".into()), None).unwrap();
        assert_eq!(req.id, Some(7));
        assert_eq!(req.label.as_deref(), Some("Portugal"));
    }

    #[test]
    fn build_pick_request_rejects_empty_label() {
        let err = build_pick_request(None, Some("   ".into()), None).unwrap_err();
        assert!(err.to_string().contains("label must not be empty"));
    }
}
