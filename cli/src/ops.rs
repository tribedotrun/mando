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

/// Session key used for all CLI ops sessions.
const CLI_OPS_KEY: &str = "cli";

pub(crate) async fn handle(args: OpsArgs) -> anyhow::Result<()> {
    if args.end {
        let client = DaemonClient::discover()?;
        client
            .post("/api/ops/end", &json!({"key": CLI_OPS_KEY}))
            .await?;
        println!("Ops session ended.");
    } else if let Some(msg) = &args.message {
        let client = DaemonClient::discover()?;
        if args.new {
            // --new with a message: force-start a fresh session.
            let result = start_session(&client, msg).await?;
            println!("{}", extract_reply(&result));
        } else {
            // Try follow-up; if no session exists, start one automatically.
            let result = match client
                .post(
                    "/api/ops/message",
                    &json!({"key": CLI_OPS_KEY, "message": msg}),
                )
                .await
            {
                Ok(resp) => resp,
                Err(_) => start_session(&client, msg).await?,
            };
            println!("{}", extract_reply(&result));
        }
    } else if args.new {
        // --new without a message: start session with a generic prompt.
        let client = DaemonClient::discover()?;
        start_session(&client, "Ready for ops tasks.").await?;
        println!("Ops session started.");
    } else {
        println!("Usage: mando ops \"your message\"");
        println!("       mando ops --new    # start new session");
        println!("       mando ops --end    # end session");
    }
    Ok(())
}

async fn start_session(client: &DaemonClient, prompt: &str) -> anyhow::Result<serde_json::Value> {
    client
        .post(
            "/api/ops/start",
            &json!({"key": CLI_OPS_KEY, "prompt": prompt}),
        )
        .await
}

fn extract_reply(result: &serde_json::Value) -> String {
    result["result_text"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default())
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
