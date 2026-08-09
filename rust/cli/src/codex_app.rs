//! `mando codex` — Codex desktop-app account swap CLI (thin dispatch).
//!
//! Swaps a pooled Codex credential into the ChatGPT desktop app's shared
//! `~/.codex/auth.json` slot, and back. See `codex_app_swap` for the
//! orchestration and `codex_app_process` for macOS process control.

use clap::{Args, Subcommand};

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
    match args.command {
        CodexCommand::Use { label } => crate::codex_app_swap::handle_app_use(label).await,
        CodexCommand::Restore => crate::codex_app_swap::handle_app_restore().await,
        CodexCommand::Status { json } => crate::codex_app_swap::handle_app_status(json).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(clap::Subcommand)]
    enum TestCommand {
        Codex(CodexArgs),
    }

    #[test]
    fn parse_app_use() {
        let cli = TestCli::try_parse_from(["test", "codex", "app-use", "PT"]).unwrap();
        match cli.cmd {
            TestCommand::Codex(args) => match args.command {
                CodexCommand::Use { label } => assert_eq!(label, "PT"),
                _ => panic!("expected Use"),
            },
        }
    }

    #[test]
    fn parse_app_restore() {
        let cli = TestCli::try_parse_from(["test", "codex", "app-restore"]).unwrap();
        match cli.cmd {
            TestCommand::Codex(args) => {
                assert!(matches!(args.command, CodexCommand::Restore));
            }
        }
    }

    #[test]
    fn parse_app_status() {
        let cli = TestCli::try_parse_from(["test", "codex", "app-status"]).unwrap();
        match cli.cmd {
            TestCommand::Codex(args) => match args.command {
                CodexCommand::Status { json } => assert!(!json),
                _ => panic!("expected Status"),
            },
        }
    }

    #[test]
    fn parse_app_status_json() {
        let cli = TestCli::try_parse_from(["test", "codex", "app-status", "--json"]).unwrap();
        match cli.cmd {
            TestCommand::Codex(args) => match args.command {
                CodexCommand::Status { json } => assert!(json),
                _ => panic!("expected Status"),
            },
        }
    }
}
