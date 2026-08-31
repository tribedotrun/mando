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

fn session_category(value: Option<&str>) -> anyhow::Result<Option<api_types::SessionCategory>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let category = match value {
        "workers" => api_types::SessionCategory::Workers,
        "clarifier" => api_types::SessionCategory::Clarifier,
        "captain-review" => api_types::SessionCategory::CaptainReview,
        "captain-ops" => api_types::SessionCategory::CaptainOps,
        "scout" => api_types::SessionCategory::Scout,
        "rebase" => api_types::SessionCategory::Rebase,
        other => anyhow::bail!(
            "unsupported session caller '{other}' (use workers, clarifier, captain-review, captain-ops, scout, rebase)"
        ),
    };
    Ok(Some(category))
}

#[derive(Args)]
pub(crate) struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,

    /// Show only last N sessions
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub last: Option<u32>,
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
    let caller = session_category(args.caller.as_deref())?;

    let entries: Vec<SessionRow> = if let Some(task_id) = args.task {
        let result = client
            .get_tasks_by_id_sessions(
                &api_types::TaskIdParams { id: task_id },
                &api_types::SessionsQuery {
                    page: None,
                    per_page: None,
                    category: None,
                    caller,
                    status: None,
                },
            )
            .await?;
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
        let result = client
            .get_sessions(&api_types::SessionsQuery {
                page: None,
                per_page: args.last,
                category: None,
                caller,
                status: None,
            })
            .await?;
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
    let text = client
        .get_sessions_by_id_stream(
            &api_types::SessionIdParams {
                id: session_id.to_string(),
            },
            &api_types::SessionStreamQuery {
                types: (!types.is_empty()).then(|| types.join(",")),
            },
        )
        .await?;
    print!("{text}");
    Ok(())
}

async fn handle_transcript(session_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get_sessions_by_id_events(&api_types::SessionIdParams {
            id: session_id.to_string(),
        })
        .await?;
    let markdown = crate::transcript_render::events_to_markdown(&result.events);
    print!("{markdown}");
    Ok(())
}

async fn handle_messages(session_id: &str, last: Option<usize>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get_sessions_by_id_messages(
            &api_types::SessionIdParams {
                id: session_id.to_string(),
            },
            &api_types::SessionMessagesQuery {
                limit: last,
                offset: None,
            },
        )
        .await?;

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
    let result = client
        .get_sessions_by_id_tools(&api_types::SessionIdParams {
            id: session_id.to_string(),
        })
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
    let result = client
        .get_sessions_by_id_cost(&api_types::SessionIdParams {
            id: session_id.to_string(),
        })
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

    #[test]
    fn session_category_parses_supported_values() {
        assert_eq!(
            session_category(Some("workers")).unwrap(),
            Some(api_types::SessionCategory::Workers)
        );
        assert!(session_category(Some("unknown")).is_err());
    }
}
