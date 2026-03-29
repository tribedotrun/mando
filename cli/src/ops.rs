//! `mando ops` — multi-turn ops copilot CLI (HTTP client).

use clap::Args;
use serde_json::json;

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct OpsArgs {
    /// Message to send (omit for new session)
    pub message: Option<String>,
    /// Start a new ops session
    #[arg(long)]
    pub new: bool,
    /// End the current ops session
    #[arg(long)]
    pub end: bool,
}

pub(crate) async fn handle(args: OpsArgs) -> anyhow::Result<()> {
    if args.end {
        let client = DaemonClient::discover()?;
        client.post("/api/ops/end", &json!({})).await?;
        println!("Ops session ended.");
        return Ok(());
    }
    if args.new {
        let client = DaemonClient::discover()?;
        client.post("/api/ops/new", &json!({})).await?;
        println!("Starting new ops session...");
        return Ok(());
    }
    match &args.message {
        Some(msg) => {
            let client = DaemonClient::discover()?;
            let body = json!({"message": msg});
            let result = client.post("/api/ops/message", &body).await?;
            let reply = result["reply"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| serde_json::to_string_pretty(&result).unwrap_or_default());
            println!("{reply}");
        }
        None => {
            println!("Usage: mando ops \"your message\"");
            println!("       mando ops --new    # start new session");
            println!("       mando ops --end    # end session");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(clap::Subcommand)]
    enum TestCmd {
        Ops(OpsArgs),
    }

    #[test]
    fn parse_ops_message() {
        let cli = TestCli::try_parse_from(["test", "ops", "check disk space"]).unwrap();
        match cli.cmd {
            TestCmd::Ops(args) => {
                assert_eq!(args.message.as_deref(), Some("check disk space"));
                assert!(!args.new);
                assert!(!args.end);
            }
        }
    }

    #[test]
    fn parse_ops_new() {
        let cli = TestCli::try_parse_from(["test", "ops", "--new"]).unwrap();
        match cli.cmd {
            TestCmd::Ops(args) => {
                assert!(args.new);
                assert!(args.message.is_none());
            }
        }
    }

    #[test]
    fn parse_ops_end() {
        let cli = TestCli::try_parse_from(["test", "ops", "--end"]).unwrap();
        match cli.cmd {
            TestCmd::Ops(args) => {
                assert!(args.end);
            }
        }
    }
}
