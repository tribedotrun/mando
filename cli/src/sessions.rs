//! `mando sessions` — CC session history CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,

    /// Show only last N sessions
    #[arg(long)]
    pub last: Option<usize>,
    /// Filter by caller (e.g. "captain", "clarifier")
    #[arg(long)]
    pub caller: Option<String>,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub(crate) enum SessionsCommand {
    /// Show transcript for a session
    Transcript {
        /// Session ID
        session_id: String,
    },
    /// Show parsed messages for a session
    Messages {
        /// Session ID
        session_id: String,
        /// Show only last N messages
        #[arg(long)]
        last: Option<usize>,
    },
    /// Show tool usage summary for a session
    Tools {
        /// Session ID
        session_id: String,
    },
    /// Show cost breakdown for a session
    Cost {
        /// Session ID
        session_id: String,
    },
}

pub(crate) async fn handle(args: SessionsArgs) -> anyhow::Result<()> {
    if let Some(cmd) = &args.command {
        return match cmd {
            SessionsCommand::Transcript { session_id } => handle_transcript(session_id).await,
            SessionsCommand::Messages { session_id, last } => {
                handle_messages(session_id, *last).await
            }
            SessionsCommand::Tools { session_id } => handle_tools(session_id).await,
            SessionsCommand::Cost { session_id } => handle_cost(session_id).await,
        };
    }

    let client = DaemonClient::discover()?;
    let mut path = "/api/sessions".to_string();
    let mut params = vec![];
    if let Some(n) = args.last {
        params.push(format!("last={n}"));
    }
    if let Some(ref caller) = args.caller {
        params.push(format!("caller={caller}"));
    }
    if !params.is_empty() {
        path = format!("{path}?{}", params.join("&"));
    }

    let result = client.get(&path).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    let empty = vec![];
    let entries = result["sessions"].as_array().unwrap_or(&empty);

    println!(
        "{:<38}  {:<20}  {:<12}  {:>8}  STATUS",
        "SESSION_ID", "DATE", "CALLER", "COST"
    );
    println!("{}", "-".repeat(90));

    for entry in entries {
        let session_id = entry["session_id"].as_str().unwrap_or("?");
        let ts = entry["ts"].as_str().unwrap_or("?");
        let caller = entry["caller"].as_str().unwrap_or("?");
        let cost = entry["cost_usd"]
            .as_f64()
            .map(|c| format!("${c:.3}"))
            .unwrap_or_else(|| "-".into());
        let status = entry["status"].as_str().unwrap_or("?");
        println!("{session_id:<38}  {ts:<20}  {caller:<12}  {cost:>8}  {status}");
    }

    println!("\n{} session(s)", entries.len());
    Ok(())
}

async fn handle_messages(session_id: &str, last: Option<usize>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut path = format!("/api/sessions/{session_id}/messages");
    if let Some(n) = last {
        path = format!("{path}?limit={n}");
    }
    let result = client.get(&path).await?;
    let empty = vec![];
    let messages = result.as_array().unwrap_or(&empty);

    for msg in messages {
        let role = msg["role"].as_str().unwrap_or("?");
        let text = msg["text"].as_str().unwrap_or("");
        let tool_calls = msg["tool_calls"].as_array();

        let prefix = if role == "user" { "Human" } else { "Assistant" };
        println!("--- {prefix} ---");
        if !text.is_empty() {
            let truncated = if text.len() > 500 {
                let end = text.floor_char_boundary(500);
                format!("{}...", &text[..end])
            } else {
                text.to_string()
            };
            println!("{truncated}");
        }
        if let Some(tools) = tool_calls {
            for tc in tools {
                let name = tc["name"].as_str().unwrap_or("?");
                println!("  [tool: {name}]");
            }
        }
        println!();
    }

    println!("{} message(s)", messages.len());
    Ok(())
}

async fn handle_tools(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get(&format!("/api/sessions/{session_id}/tools"))
        .await?;
    let empty = vec![];
    let tools = result.as_array().unwrap_or(&empty);

    println!("{:<20}  {:>6}  {:>6}", "TOOL", "CALLS", "ERRORS");
    println!("{}", "-".repeat(40));
    for t in tools {
        let name = t["name"].as_str().unwrap_or("?");
        let calls = t["call_count"].as_u64().unwrap_or(0);
        let errors = t["error_count"].as_u64().unwrap_or(0);
        println!("{name:<20}  {calls:>6}  {errors:>6}");
    }

    println!("\n{} tool type(s)", tools.len());
    Ok(())
}

async fn handle_cost(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get(&format!("/api/sessions/{session_id}/cost"))
        .await?;

    let input = result["total_input_tokens"].as_u64().unwrap_or(0);
    let output = result["total_output_tokens"].as_u64().unwrap_or(0);
    let cache_read = result["total_cache_read_tokens"].as_u64().unwrap_or(0);
    let cache_create = result["total_cache_creation_tokens"].as_u64().unwrap_or(0);
    let turns = result["turn_count"].as_u64().unwrap_or(0);
    let cost = result["total_cost_usd"]
        .as_f64()
        .map(|c| format!("${c:.4}"))
        .unwrap_or_else(|| "-".into());

    println!("Input tokens:    {input:>12}");
    println!("Output tokens:   {output:>12}");
    println!("Cache read:      {cache_read:>12}");
    println!("Cache creation:  {cache_create:>12}");
    println!("Turns:           {turns:>12}");
    println!("Total cost:      {cost:>12}");
    Ok(())
}

async fn handle_transcript(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get(&format!("/api/sessions/{session_id}/transcript"))
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
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
        Sessions(SessionsArgs),
    }

    #[test]
    fn parse_sessions_default() {
        let cli = TestCli::try_parse_from(["test", "sessions"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => {
                assert!(args.last.is_none());
                assert!(args.caller.is_none());
                assert!(!args.json);
                assert!(args.command.is_none());
            }
        }
    }

    #[test]
    fn parse_sessions_last() {
        let cli = TestCli::try_parse_from(["test", "sessions", "--last", "10"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => {
                assert_eq!(args.last, Some(10));
            }
        }
    }

    #[test]
    fn parse_sessions_caller() {
        let cli = TestCli::try_parse_from(["test", "sessions", "--caller", "captain"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => {
                assert_eq!(args.caller.as_deref(), Some("captain"));
            }
        }
    }

    #[test]
    fn parse_sessions_json() {
        let cli = TestCli::try_parse_from(["test", "sessions", "--json"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => {
                assert!(args.json);
            }
        }
    }

    #[test]
    fn parse_sessions_transcript() {
        let cli = TestCli::try_parse_from(["test", "sessions", "transcript", "abc-123"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => match args.command {
                Some(SessionsCommand::Transcript { session_id }) => {
                    assert_eq!(session_id, "abc-123");
                }
                _ => panic!("expected Transcript"),
            },
        }
    }
}
