//! `mando sessions` — CC session history CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

fn session_status_label(status: api_types::SessionStatus) -> &'static str {
    match status {
        api_types::SessionStatus::Running => "running",
        api_types::SessionStatus::Stopped => "stopped",
        api_types::SessionStatus::Failed => "failed",
    }
}

struct SessionRow {
    session_id: String,
    timestamp: String,
    caller: String,
    cost_usd: Option<f64>,
    credential_label: Option<String>,
    status: api_types::SessionStatus,
}

#[derive(Args)]
pub(crate) struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,

    /// Show only last N sessions
    #[arg(long)]
    pub last: Option<usize>,
    /// Filter by caller group (e.g. "workers", "captain-review", "clarifier")
    #[arg(long)]
    pub caller: Option<String>,
    /// Filter by task ID (combinable with --caller; conflicts with --last)
    #[arg(long, conflicts_with = "last")]
    pub task: Option<i64>,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub(crate) enum SessionsCommand {
    /// Show markdown transcript for a session (human-readable)
    Transcript {
        /// Session ID
        session_id: String,
    },
    /// Show raw JSONL stream for a session (agent-readable)
    Stream {
        /// Session ID
        session_id: String,
        /// Include only these event types (repeatable, e.g. --type user --type assistant)
        #[arg(long = "type", value_name = "TYPE")]
        types: Vec<String>,
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
            SessionsCommand::Stream { session_id, types } => handle_stream(session_id, types).await,
            SessionsCommand::Messages { session_id, last } => {
                handle_messages(session_id, *last).await
            }
            SessionsCommand::Tools { session_id } => handle_tools(session_id).await,
            SessionsCommand::Cost { session_id } => handle_cost(session_id).await,
        };
    }

    let client = DaemonClient::discover()?;

    let entries: Vec<SessionRow> = if let Some(task_id) = args.task {
        let mut path = format!("/api/tasks/{task_id}/sessions");
        if let Some(ref caller) = args.caller {
            path = format!("{path}?caller={caller}");
        }
        let result: api_types::ItemSessionsResponse = client.get_json(&path).await?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        result
            .sessions
            .into_iter()
            .map(|entry| SessionRow {
                session_id: entry.session_id,
                timestamp: entry.started_at,
                caller: entry.caller,
                cost_usd: entry.cost_usd,
                credential_label: None,
                status: entry.status,
            })
            .collect()
    } else {
        let mut params = vec![];
        if let Some(n) = args.last {
            params.push(format!("last={n}"));
        }
        if let Some(ref caller) = args.caller {
            params.push(format!("caller={caller}"));
        }
        let path = if params.is_empty() {
            "/api/sessions".to_string()
        } else {
            format!("/api/sessions?{}", params.join("&"))
        };
        let result: api_types::SessionsListResponse = client.get_json(&path).await?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        result
            .sessions
            .into_iter()
            .map(|entry| SessionRow {
                session_id: entry.session_id,
                timestamp: entry.created_at,
                caller: entry.caller,
                cost_usd: entry.cost_usd,
                credential_label: entry.credential_label,
                status: entry.status,
            })
            .collect()
    };

    println!(
        "{:<38}  {:<20}  {:<12}  {:>8}  {:<12}  STATUS",
        "SESSION_ID", "DATE", "CALLER", "COST", "CREDENTIAL"
    );
    println!("{}", "-".repeat(105));

    for entry in &entries {
        let ts = if entry.timestamp.is_empty() {
            "?".to_string()
        } else {
            entry.timestamp[..entry.timestamp.len().min(16)].to_string()
        };
        let cost = entry
            .cost_usd
            .map(|c| format!("${c:.3}"))
            .unwrap_or_else(|| "-".into());
        let credential = entry.credential_label.as_deref().unwrap_or("-");
        let status = session_status_label(entry.status);
        println!(
            "{:<38}  {:<20}  {:<12}  {:>8}  {:<12}  {}",
            entry.session_id, ts, entry.caller, cost, credential, status
        );
    }

    println!("\n{} session(s)", entries.len());
    Ok(())
}

async fn handle_stream(session_id: &str, types: &[String]) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let path = if types.is_empty() {
        format!("/api/sessions/{session_id}/stream")
    } else {
        format!(
            "/api/sessions/{session_id}/stream?types={}",
            types.join(",")
        )
    };
    let text = client.get_text(&path).await?;
    print!("{text}");
    Ok(())
}

async fn handle_transcript(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::TranscriptResponse = client
        .get_json(&format!("/api/sessions/{session_id}/transcript"))
        .await?;
    print!("{}", result.markdown);
    Ok(())
}

async fn handle_messages(session_id: &str, last: Option<usize>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut path = format!("/api/sessions/{session_id}/messages");
    if let Some(n) = last {
        path = format!("{path}?limit={n}");
    }
    let result: api_types::SessionMessagesResponse = client.get_json(&path).await?;

    for msg in &result.messages {
        let prefix = if msg.role == "user" {
            "Human"
        } else {
            "Assistant"
        };
        println!("--- {prefix} ---");
        if !msg.text.is_empty() {
            let truncated = if msg.text.len() > 500 {
                let end = msg.text.floor_char_boundary(500);
                format!("{}...", &msg.text[..end])
            } else {
                msg.text.clone()
            };
            println!("{truncated}");
        }
        for tc in &msg.tool_calls {
            println!("  [tool: {}]", tc.name);
        }
        println!();
    }

    println!("{} message(s)", result.messages.len());
    Ok(())
}

async fn handle_tools(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::SessionToolUsageResponse = client
        .get_json(&format!("/api/sessions/{session_id}/tools"))
        .await?;

    println!("{:<20}  {:>6}  {:>6}", "TOOL", "CALLS", "ERRORS");
    println!("{}", "-".repeat(40));
    for t in &result.tools {
        println!("{:<20}  {:>6}  {:>6}", t.name, t.call_count, t.error_count);
    }

    println!("\n{} tool type(s)", result.tools.len());
    Ok(())
}

async fn handle_cost(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::SessionCostResponse = client
        .get_json(&format!("/api/sessions/{session_id}/cost"))
        .await?;
    let cost = result
        .cost
        .total_cost_usd
        .map(|c| format!("${c:.4}"))
        .unwrap_or_else(|| "-".into());

    println!("Input tokens:    {:>12}", result.cost.total_input_tokens);
    println!("Output tokens:   {:>12}", result.cost.total_output_tokens);
    println!(
        "Cache read:      {:>12}",
        result.cost.total_cache_read_tokens
    );
    println!(
        "Cache creation:  {:>12}",
        result.cost.total_cache_creation_tokens
    );
    println!("Turns:           {:>12}", result.cost.turn_count);
    println!("Total cost:      {cost:>12}");
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
                assert!(args.task.is_none());
            }
        }
    }

    #[test]
    fn parse_sessions_task_and_caller() {
        let cli = TestCli::try_parse_from([
            "test",
            "sessions",
            "--task",
            "42",
            "--caller",
            "captain-review",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => {
                assert_eq!(args.task, Some(42));
                assert_eq!(args.caller.as_deref(), Some("captain-review"));
            }
        }
    }

    #[test]
    fn parse_sessions_transcript() {
        let cli = TestCli::try_parse_from(["test", "sessions", "transcript", "sess-1"]).unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => match args.command.unwrap() {
                SessionsCommand::Transcript { session_id } => assert_eq!(session_id, "sess-1"),
                _ => panic!("expected Transcript"),
            },
        }
    }

    #[test]
    fn parse_sessions_stream_types() {
        let cli = TestCli::try_parse_from([
            "test",
            "sessions",
            "stream",
            "sess-1",
            "--type",
            "user",
            "--type",
            "assistant",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Sessions(args) => match args.command.unwrap() {
                SessionsCommand::Stream { session_id, types } => {
                    assert_eq!(session_id, "sess-1");
                    assert_eq!(types, vec!["user", "assistant"]);
                }
                _ => panic!("expected Stream"),
            },
        }
    }
}
