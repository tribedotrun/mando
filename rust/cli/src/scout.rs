//! `mando scout` — scout management CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct ScoutArgs {
    #[command(subcommand)]
    pub command: ScoutCommand,
}

#[derive(Subcommand)]
pub(crate) enum ScoutCommand {
    /// List scout items (compact)
    #[command(name = "simplelist")]
    SimpleList {
        /// Filter by status (pending, processed, saved, archived)
        #[arg(long)]
        status: Option<String>,
    },
    /// Add a URL to scout
    Add {
        /// URL to add
        url: String,
        /// Title (optional, auto-detected)
        #[arg(short = 't')]
        title: Option<String>,
    },
    /// Show a scout item with summary
    Show {
        /// Item ID
        id: i64,
    },
    /// Delete a scout item
    Delete {
        /// Item ID
        id: i64,
    },
    /// Delete multiple scout items
    BulkDelete {
        /// Item IDs
        ids: Vec<i64>,
    },
    /// Update item status
    Status {
        /// Item ID
        id: i64,
        /// New status
        status: String,
    },
    /// Update status for multiple items
    BulkStatus {
        /// New status
        status: String,
        /// Item IDs
        ids: Vec<i64>,
    },
    /// List items with inline summaries
    List {
        /// Filter by status (pending, processed, saved, archived)
        #[arg(long)]
        status: Option<String>,
    },
    /// Mark item as saved (shortcut for status <id> saved)
    Save {
        /// Item ID
        id: i64,
    },
    /// Mark item as archived (shortcut for status <id> archived)
    Archive {
        /// Item ID
        id: i64,
    },
    /// Show full article for a scout item
    Read {
        /// Item ID
        id: i64,
    },
    /// Ask a question about a scout article
    Ask {
        /// Item ID
        id: i64,
        /// Existing session ID for follow-up questions
        #[arg(long)]
        session: Option<String>,
        /// Question to ask
        question: Vec<String>,
    },
    /// Research a topic and discover links (auto-processed server-side).
    Research {
        /// Topic to research
        topic: Vec<String>,
    },
    /// Create a task from a scout item
    Act {
        /// Item ID
        id: i64,
        /// Project slug
        project: String,
        /// Optional operator prompt
        prompt: Vec<String>,
    },
    /// Publish the extracted article and print the public URL
    Publish {
        /// Item ID
        id: i64,
    },
    /// Show CC sessions linked to a scout item
    Sessions {
        /// Item ID
        id: i64,
    },
}

pub(crate) async fn handle(args: ScoutArgs) -> anyhow::Result<()> {
    match args.command {
        ScoutCommand::SimpleList { status } => handle_list(status.as_deref()).await,
        ScoutCommand::Add { url, title } => handle_add(&url, title.as_deref()).await,
        ScoutCommand::Show { id } => handle_show(id).await,
        ScoutCommand::Delete { id } => handle_delete(id).await,
        ScoutCommand::BulkDelete { ids } => handle_bulk_delete(&ids).await,
        ScoutCommand::Status { id, status } => handle_status(id, &status).await,
        ScoutCommand::BulkStatus { status, ids } => handle_bulk_status(&status, &ids).await,
        ScoutCommand::List { status } => handle_list_with_summaries(status.as_deref()).await,
        ScoutCommand::Save { id } => handle_status(id, "saved").await,
        ScoutCommand::Archive { id } => handle_status(id, "archived").await,
        ScoutCommand::Read { id } => handle_read(id).await,
        ScoutCommand::Ask {
            id,
            session,
            question,
        } => handle_ask(id, session.as_deref(), &question.join(" ")).await,
        ScoutCommand::Research { topic } => handle_research(&topic.join(" ")).await,
        ScoutCommand::Act {
            id,
            project,
            prompt,
        } => handle_act(id, &project, &prompt.join(" ")).await,
        ScoutCommand::Publish { id } => handle_publish(id).await,
        ScoutCommand::Sessions { id } => handle_sessions(id).await,
    }
}

async fn handle_list(status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut params = vec!["per_page=10000".to_string()];
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    let path = format!("/api/scout/items?{}", params.join("&"));
    let result: api_types::ScoutResponse = client.get_json(&path).await?;

    println!("{:>4}  {:<10}  {:<12}  TITLE", "ID", "STATUS", "TYPE");
    println!("{}", "-".repeat(70));

    for item in &result.items {
        let title = item.title.as_deref().unwrap_or(item.url.as_str());
        println!(
            "{:>4}  {:<10}  {:<12}  {}",
            item.id,
            item.status,
            item.item_type.as_deref().unwrap_or("?"),
            title
        );
    }

    println!("\n{} item(s)", result.total);
    Ok(())
}

async fn handle_add(url: &str, title: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::ScoutAddResponse = client
        .post_json(
            "/api/scout/items",
            &api_types::ScoutAddRequest {
                url: url.to_string(),
                title: title.map(str::to_string),
            },
        )
        .await?;
    println!("Added scout item #{}: {url}", result.id);
    Ok(())
}

async fn handle_show(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::ScoutItem = client.get_json(&format!("/api/scout/items/{id}")).await?;

    println!(
        "Item #{id}: {}",
        result.title.as_deref().unwrap_or("(no title)")
    );
    println!("  URL:    {}", result.url);
    println!("  Status: {}", result.status);
    if let Some(summary) = result.summary.as_deref() {
        println!("\nSummary:\n{summary}");
    } else {
        println!("\n(No summary available)");
    }
    Ok(())
}

fn lifecycle_command_for_status(
    status: &str,
) -> anyhow::Result<api_types::ScoutItemLifecycleCommand> {
    match status {
        "pending" => Ok(api_types::ScoutItemLifecycleCommand::MarkPending),
        "processed" => Ok(api_types::ScoutItemLifecycleCommand::MarkProcessed),
        "saved" => Ok(api_types::ScoutItemLifecycleCommand::Save),
        "archived" => Ok(api_types::ScoutItemLifecycleCommand::Archive),
        other => anyhow::bail!(
            "unsupported scout lifecycle target '{other}' (use pending, processed, saved, archived)"
        ),
    }
}

async fn handle_delete(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .delete_json::<api_types::ScoutDeleteResponse>(&format!("/api/scout/items/{id}"))
        .await?;
    println!("Deleted scout item #{id}.");
    Ok(())
}

async fn handle_bulk_delete(ids: &[i64]) -> anyhow::Result<()> {
    if ids.is_empty() {
        anyhow::bail!("provide at least one item ID");
    }
    let client = DaemonClient::discover()?;
    let result: api_types::ScoutBulkDeleteResponse = client
        .post_json(
            "/api/scout/bulk-delete",
            &api_types::ScoutBulkDeleteRequest { ids: ids.to_vec() },
        )
        .await?;
    println!("Deleted {} scout item(s).", result.deleted);
    Ok(())
}

async fn handle_status(id: i64, status: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let command = lifecycle_command_for_status(status)?;
    client
        .patch_json::<api_types::BoolOkResponse, _>(
            &format!("/api/scout/items/{id}"),
            &api_types::ScoutLifecycleCommandRequest { command },
        )
        .await?;
    println!("Updated item #{id} status to '{status}'.");
    Ok(())
}

async fn handle_bulk_status(status: &str, ids: &[i64]) -> anyhow::Result<()> {
    if ids.is_empty() {
        anyhow::bail!("provide at least one item ID");
    }
    let client = DaemonClient::discover()?;
    let command = lifecycle_command_for_status(status)?;
    let result: api_types::ScoutBulkUpdateResponse = client
        .post_json(
            "/api/scout/bulk",
            &api_types::ScoutBulkCommandRequest {
                ids: ids.to_vec(),
                command,
            },
        )
        .await?;
    println!("Updated {} scout item(s) to '{status}'.", result.updated);
    Ok(())
}

async fn handle_list_with_summaries(status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut params = vec!["per_page=10000".to_string()];
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    let path = format!("/api/scout/items?{}", params.join("&"));
    let result: api_types::ScoutResponse = client.get_json(&path).await?;

    for item in &result.items {
        let scores = match (item.relevance, item.quality) {
            (Some(r), Some(q)) => format!(" R:{r}·Q:{q}"),
            _ => String::new(),
        };
        let title = item.title.as_deref().unwrap_or(item.url.as_str());
        println!("#{} [{}] {title}{scores}", item.id, item.status);

        match client
            .get_json::<api_types::ScoutItem>(&format!("/api/scout/items/{}", item.id))
            .await
        {
            Ok(full) => {
                if let Some(summary) = full.summary.as_deref() {
                    for line in summary.lines().take(3) {
                        println!("  {line}");
                    }
                }
            }
            Err(e) => {
                eprintln!("  (summary unavailable: {e})");
            }
        }
        println!();
    }

    println!("{} item(s)", result.total);
    Ok(())
}

async fn handle_read(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::ScoutArticleResponse = client
        .get_json(&format!("/api/scout/items/{id}/article"))
        .await?;

    println!("# {}\n", result.title.as_deref().unwrap_or("(no title)"));
    if let Some(article) = result.article.as_deref() {
        println!("{article}");
    } else {
        println!("(No article content available — process item first)");
    }
    if let Some(telegraph_url) = result.telegraph_url.as_deref() {
        println!("\nPublished URL: {telegraph_url}");
    }
    Ok(())
}

async fn handle_ask(id: i64, session: Option<&str>, question: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::AskResponse = client
        .post_json(
            "/api/scout/ask",
            &api_types::ScoutAskRequest {
                id,
                question: question.to_string(),
                session_id: session.map(str::to_string),
            },
        )
        .await?;

    if result.answer.is_empty() {
        println!("(No answer returned)");
    } else {
        println!("{}", result.answer);
    }
    if let Some(session_id) = result.session_id.as_deref() {
        println!("\nSession: {session_id}");
    }
    Ok(())
}

async fn handle_research(topic: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    println!("Researching: {topic}...");
    let result: api_types::ResearchStartResponse = client
        .post_json(
            "/api/scout/research",
            &api_types::ScoutResearchRequest {
                topic: topic.to_string(),
                process: Some(true),
            },
        )
        .await?;
    let run_id = result.run_id;
    println!("Research started (run #{run_id})\n");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let run: api_types::ScoutResearchRun = client
            .get_json(&format!("/api/scout/research/{run_id}"))
            .await?;
        match run.status.as_str() {
            "done" => {
                println!("Research complete: {} link(s) added.", run.added_count);
                return Ok(());
            }
            "failed" => {
                let error = run.error.as_deref().unwrap_or("unknown");
                anyhow::bail!("Research failed: {error}");
            }
            _ => {
                print!(".");
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        }
    }
}

async fn handle_act(id: i64, project: &str, prompt: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::ActResponse = client
        .post_json(
            &format!("/api/scout/items/{id}/act"),
            &api_types::ScoutActRequest {
                project: project.to_string(),
                prompt: (!prompt.is_empty()).then(|| prompt.to_string()),
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_publish(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::TelegraphPublishResponse = client
        .post_no_body(&format!("/api/scout/items/{id}/telegraph"))
        .await?;
    println!("{}", result.url);
    Ok(())
}

async fn handle_sessions(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: Vec<api_types::ScoutItemSession> = client
        .get_json(&format!("/api/scout/items/{id}/sessions"))
        .await?;
    if result.is_empty() {
        println!("No sessions linked to scout item #{id}.");
        return Ok(());
    }
    println!("Scout item #{id} sessions");
    println!("{}", "-".repeat(60));
    for session in &result {
        println!(
            "{:<38}  {:<12}  {:<10}  {}",
            session.session_id, session.caller, session.status, session.created_at
        );
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
        Scout(ScoutArgs),
    }

    #[test]
    fn parse_scout_simplelist() {
        let cli = TestCli::try_parse_from(["test", "scout", "simplelist"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::SimpleList { status } => assert!(status.is_none()),
                _ => panic!("expected SimpleList"),
            },
        }
    }

    #[test]
    fn parse_scout_simplelist_status() {
        let cli = TestCli::try_parse_from(["test", "scout", "simplelist", "--status", "pending"])
            .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::SimpleList { status } => {
                    assert_eq!(status.as_deref(), Some("pending"));
                }
                _ => panic!("expected SimpleList"),
            },
        }
    }

    #[test]
    fn parse_scout_add() {
        let cli = TestCli::try_parse_from([
            "test",
            "scout",
            "add",
            "https://example.com",
            "-t",
            "Example",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Add { url, title } => {
                    assert_eq!(url, "https://example.com");
                    assert_eq!(title.as_deref(), Some("Example"));
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_scout_bulk_delete() {
        let cli = TestCli::try_parse_from(["test", "scout", "bulk-delete", "7", "8"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::BulkDelete { ids } => assert_eq!(ids, vec![7, 8]),
                _ => panic!("expected BulkDelete"),
            },
        }
    }

    #[test]
    fn parse_scout_ask_with_session() {
        let cli = TestCli::try_parse_from([
            "test",
            "scout",
            "ask",
            "42",
            "--session",
            "sess-1",
            "What",
            "changed?",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Ask {
                    id,
                    session,
                    question,
                } => {
                    assert_eq!(id, 42);
                    assert_eq!(session.as_deref(), Some("sess-1"));
                    assert_eq!(question.join(" "), "What changed?");
                }
                _ => panic!("expected Ask"),
            },
        }
    }

    #[test]
    fn parse_scout_act() {
        let cli = TestCli::try_parse_from([
            "test", "scout", "act", "42", "sandbox", "Focus", "on", "tests",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Act {
                    id,
                    project,
                    prompt,
                } => {
                    assert_eq!(id, 42);
                    assert_eq!(project, "sandbox");
                    assert_eq!(prompt.join(" "), "Focus on tests");
                }
                _ => panic!("expected Act"),
            },
        }
    }

    #[test]
    fn parse_scout_sessions() {
        let cli = TestCli::try_parse_from(["test", "scout", "sessions", "9"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Sessions { id } => assert_eq!(id, 9),
                _ => panic!("expected Sessions"),
            },
        }
    }
}
