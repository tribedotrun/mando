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
//! It prints `export CLAUDE_CODE_OAUTH_TOKEN='...'` on success, and
//! `unset CLAUDE_CODE_OAUTH_TOKEN` on every fallback path (daemon down,
//! no usable credential, transport error). The wrapper eval's stdout, so
//! the explicit `unset` ensures a stale token from a prior successful
//! pick can't leak into a later session — without it, "fall through to
//! ambient login" would be a lie when the shell already had the var set.

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
    /// Pick the best-available credential right now and emit either
    /// `export CLAUDE_CODE_OAUTH_TOKEN='<token>'` (success) or
    /// `unset CLAUDE_CODE_OAUTH_TOKEN` (any fallback path) so
    /// `eval "$(mando credentials pick)"` always leaves the shell in a
    /// correct state — never with a stale token from a prior pick.
    Pick,
}

pub(crate) async fn handle(args: CredentialsArgs) -> Result<()> {
    match args.command {
        CredentialsCommand::List => handle_list().await,
        CredentialsCommand::Pick => handle_pick().await,
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
        "{:<4} {:<24} {:<14} {:>6}  TOKEN",
        "ID", "LABEL", "STATE", "5H%"
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
        println!(
            "{:<4} {:<24} {:<14} {:>6}  {}",
            cred.id, cred.label, state, util, cred.token_masked
        );
    }
    Ok(())
}

/// Print one of:
/// - `export CLAUDE_CODE_OAUTH_TOKEN='<token>'` when a credential was picked, or
/// - `unset CLAUDE_CODE_OAUTH_TOKEN` on any fallback path (daemon down, no
///   pick, transport error).
///
/// Why always emit `unset` on the fallback paths: the wrapper eval's our
/// stdout in the user's shell, so an earlier successful pick leaves
/// `CLAUDE_CODE_OAUTH_TOKEN` set indefinitely. Without an explicit clear,
/// the next `claude` invocation after a daemon stop / cooldown would
/// inherit a stale token instead of falling through to ambient login —
/// the wrapper's advertised behavior.
///
/// Always exits 0 so `eval "$(...)"` never breaks the user's session.
async fn handle_pick() -> Result<()> {
    let Ok(client) = DaemonClient::discover() else {
        emit_unset();
        return Ok(());
    };

    let result: api_types::CredentialPickResponse =
        match client.post_no_body(paths::CREDENTIALS_PICK).await {
            Ok(r) => r,
            Err(_) => {
                emit_unset();
                return Ok(());
            }
        };

    if let Some(pick) = result.pick {
        let token = shell_single_quote(&pick.token);
        println!("export CLAUDE_CODE_OAUTH_TOKEN={token}");
        eprintln!("mando: using credential '{}' (#{})", pick.label, pick.id);
    } else {
        emit_unset();
        eprintln!(
            "mando: no credentials available (none configured, all expired, or all rate-limited); falling through to ambient login"
        );
    }
    Ok(())
}

fn emit_unset() {
    println!("unset CLAUDE_CODE_OAUTH_TOKEN");
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
        // `a'b` → `'a'\''b'` — closes, escapes, reopens. Round-trips through
        // any POSIX shell.
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn shell_single_quote_empty() {
        assert_eq!(shell_single_quote(""), "''");
    }
}
