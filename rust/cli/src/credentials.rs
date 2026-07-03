//! CLI subcommand: credential pool inspection and pick-for-shell — pure HTTP client.
//!
//! `pick` is the integration point for the iTerm2 shell wrapper:
//!
//! ```sh
//! claude() {
//!   eval "$(command mando credentials pick 2>/dev/null)" || true
//!   command claude "$@"
//! }
//! ```
//!
//! `pick --codex` mirrors that for Codex via a per-process `CODEX_HOME`
//! (picked `auth.json` + symlinks to `~/.codex` session state). ChatGPT OAuth
//! tokens are JWT-shaped and must not be passed through `CODEX_ACCESS_TOKEN`.
//!
//! ```sh
//! mdo create --agent codex   # uses codex-pooled-launch.sh under the hood
//! ```
//! `pick --codex` emits only `unset`/`export` lines. Launchers call
//! `codex-pooled-launch.sh` to pick, run `codex`, then `sync-codex`.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::gateway_paths as paths;
use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct CredentialsArgs {
    #[command(subcommand)]
    pub command: CredentialsCommand,
}

#[derive(Subcommand)]
pub(crate) enum CredentialsCommand {
    /// List stored credentials (masked tokens, current rate-limit/cooldown).
    List,
    /// Pick the best-available credential right now and emit shell exports
    /// (success) or unsets (any fallback path) so `eval "$(mando credentials pick)"`
    /// always leaves the shell in a correct state.
    Pick {
        /// Pick a Codex OAuth credential instead of Claude.
        #[arg(long)]
        codex: bool,
    },
    /// Persist refreshed tokens from `$CODEX_HOME/auth.json` back to the daemon.
    /// Called by `codex-pooled-launch.sh` after a Codex session ends.
    SyncCodex,
}

pub(crate) async fn handle(args: CredentialsArgs) -> Result<()> {
    match args.command {
        CredentialsCommand::List => handle_list().await,
        CredentialsCommand::Pick { codex } => {
            if codex {
                handle_pick_codex().await
            } else {
                handle_pick_claude().await
            }
        }
        CredentialsCommand::SyncCodex => handle_sync_codex().await,
    }
}

async fn handle_list() -> Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::CredentialsListResponse = client.get_json(paths::CREDENTIALS).await?;

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
        let state = if cred.is_expired {
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

async fn handle_pick_claude() -> Result<()> {
    let Ok(client) = DaemonClient::discover() else {
        emit_claude_unset();
        return Ok(());
    };

    let result: api_types::CredentialPickResponse =
        match client.post_no_body(paths::CREDENTIALS_PICK).await {
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
        eprintln!(
            "mando: no credentials available (none configured, all expired, or all rate-limited); falling through to ambient login"
        );
    }
    Ok(())
}

async fn handle_pick_codex() -> Result<()> {
    let Ok(client) = DaemonClient::discover() else {
        emit_codex_unset();
        return Ok(());
    };

    let result: api_types::CodexCredentialPickResponse =
        match client.post_no_body(paths::CREDENTIALS_CODEX_PICK).await {
            Ok(r) => r,
            Err(_) => {
                emit_codex_unset();
                return Ok(());
            }
        };

    if let Some(pick) = result.pick {
        match crate::credentials_codex_pick::materialize_codex_home(&pick.auth_json) {
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
                eprintln!("mando: failed to prepare Codex home: {err}");
            }
        }
    } else {
        emit_codex_unset();
        eprintln!(
            "mando: no Codex credentials available (none configured, all expired, or all rate-limited); falling through to ambient login"
        );
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
    let _: api_types::SyncCodexCredentialResponse = client
        .post_json(paths::CREDENTIALS_CODEX_SYNC, &body)
        .await?;

    if let Ok(home) = std::env::var("CODEX_HOME") {
        let path = std::path::PathBuf::from(home);
        if crate::credentials_codex_pick::is_managed_codex_home(&path) {
            crate::credentials_codex_pick::cleanup_managed_codex_home(&path)?;
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
}
