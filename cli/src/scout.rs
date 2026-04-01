//! `mando scout` — scout management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

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
    /// Research a topic and discover links
    Research {
        /// Topic to research
        topic: Vec<String>,
        /// Auto-process discovered links
        #[arg(long)]
        process: bool,
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
        ScoutCommand::Research { topic, process } => {
            handle_research(&topic.join(" "), process).await
        }
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
    let result = client.get(&path).await?;
    let items = result["items"].as_array();

    println!("{:>4}  {:<10}  {:<12}  TITLE", "ID", "STATUS", "TYPE");
    println!("{}", "-".repeat(70));

    if let Some(items) = items {
        for item in items {
            let id = item["id"].as_i64().unwrap_or(0);
            let st = item["status"].as_str().unwrap_or("?");
            let itype = item["item_type"].as_str().unwrap_or("?");
            let title = item["title"]
                .as_str()
                .unwrap_or(item["url"].as_str().unwrap_or("(no title)"));
            println!("{id:>4}  {st:<10}  {itype:<12}  {title}");
        }
    }

    let total = result["total"].as_u64().unwrap_or(0);
    println!("\n{total} item(s)");
    Ok(())
}

async fn handle_add(url: &str, title: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"url": url});
    if let Some(t) = title {
        body["title"] = json!(t);
    }
    let result = client.post("/api/scout/items", &body).await?;
    let id = result["id"].as_i64().unwrap_or(0);
    println!("Added scout item #{id}: {url}");
    Ok(())
}

async fn handle_show(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get(&format!("/api/scout/items/{id}")).await?;

    let title = result["title"].as_str().unwrap_or("(no title)");
    let url = result["url"].as_str().unwrap_or("");
    let status = result["status"].as_str().unwrap_or("?");
    let summary = result["summary"].as_str();

    println!("Item #{id}: {title}");
    println!("  URL:    {url}");
    println!("  Status: {status}");
    if let Some(s) = summary {
        println!("\nSummary:\n{s}");
    } else {
        println!("\n(No summary available)");
    }
    Ok(())
}

async fn handle_delete(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client.delete(&format!("/api/scout/items/{id}")).await?;
    println!("Deleted scout item #{id}.");
    Ok(())
}

async fn handle_bulk_delete(ids: &[i64]) -> anyhow::Result<()> {
    if ids.is_empty() {
        anyhow::bail!("provide at least one item ID");
    }
    let client = DaemonClient::discover()?;
    let result = client
        .post("/api/scout/bulk-delete", &json!({"ids": ids}))
        .await?;
    let deleted = result["deleted"].as_u64().unwrap_or(0);
    println!("Deleted {deleted} scout item(s).");
    Ok(())
}

async fn handle_status(id: i64, status: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"status": status});
    client
        .patch(&format!("/api/scout/items/{id}"), &body)
        .await?;
    println!("Updated item #{id} status to '{status}'.");
    Ok(())
}

async fn handle_bulk_status(status: &str, ids: &[i64]) -> anyhow::Result<()> {
    if ids.is_empty() {
        anyhow::bail!("provide at least one item ID");
    }
    let client = DaemonClient::discover()?;
    let result = client
        .post(
            "/api/scout/bulk",
            &json!({"ids": ids, "updates": {"status": status}}),
        )
        .await?;
    let updated = result["updated"].as_u64().unwrap_or(0);
    println!("Updated {updated} scout item(s) to '{status}'.");
    Ok(())
}

async fn handle_list_with_summaries(status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut params = vec!["per_page=10000".to_string()];
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    let path = format!("/api/scout/items?{}", params.join("&"));
    let result = client.get(&path).await?;
    let items = result["items"].as_array();

    if let Some(items) = items {
        for item in items {
            let id = item["id"].as_i64().unwrap_or(0);
            let st = item["status"].as_str().unwrap_or("?");
            let title = item["title"]
                .as_str()
                .unwrap_or(item["url"].as_str().unwrap_or("(no title)"));
            let scores = match (item["relevance"].as_i64(), item["quality"].as_i64()) {
                (Some(r), Some(q)) => format!(" R:{r}·Q:{q}"),
                _ => String::new(),
            };
            println!("#{id} [{st}] {title}{scores}");

            if let Ok(full) = client.get(&format!("/api/scout/items/{id}")).await {
                if let Some(summary) = full["summary"].as_str() {
                    for line in summary.lines().take(3) {
                        println!("  {line}");
                    }
                }
            }
            println!();
        }
    }

    let total = result["total"].as_u64().unwrap_or(0);
    println!("{total} item(s)");
    Ok(())
}

async fn handle_read(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get(&format!("/api/scout/items/{id}/article"))
        .await?;

    let title = result["title"].as_str().unwrap_or("(no title)");

    println!("# {title}\n");
    if let Some(article) = result["article"].as_str() {
        println!("{article}");
    } else {
        println!("(No article content available — process item first)");
    }
    if let Some(telegraph_url) = result["telegraphUrl"].as_str() {
        println!("\nPublished URL: {telegraph_url}");
    }
    Ok(())
}

async fn handle_ask(id: i64, session: Option<&str>, question: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"id": id, "question": question});
    if let Some(session_id) = session {
        body["session_id"] = json!(session_id);
    }
    let result = client.post("/api/scout/ask", &body).await?;

    if let Some(answer) = result["answer"].as_str() {
        println!("{answer}");
    } else if result["ok"].as_bool() == Some(true) {
        println!("(No answer returned)");
    }
    if let Some(session_id) = result["session_id"].as_str() {
        println!("\nSession: {session_id}");
    }
    Ok(())
}

async fn handle_research(topic: &str, process: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"topic": topic, "process": process});
    println!("Researching: {topic}...\n");
    let result = client.post("/api/scout/research", &body).await?;

    if let Some(links) = result["links"].as_array() {
        for (i, link) in links.iter().enumerate() {
            let url = link["url"].as_str().unwrap_or("?");
            let title = link["title"].as_str().unwrap_or("(no title)");
            let ltype = link["type"].as_str().unwrap_or("other");
            let reason = link["reason"].as_str().unwrap_or("");
            println!("{}. [{}] {}", i + 1, ltype, title);
            println!("   {url}");
            if !reason.is_empty() {
                println!("   {reason}");
            }
            println!();
        }
        let count = links.len();
        println!("{count} link(s) found.");
        if let Some(added) = result["added"].as_u64() {
            println!("{added} added to scout.");
        }
        if let Some(processed) = result["processed"].as_u64() {
            println!("{processed} processed.");
        }
    } else {
        println!("No links found.");
    }
    Ok(())
}

async fn handle_act(id: i64, project: &str, prompt: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"project": project});
    if !prompt.is_empty() {
        body["prompt"] = json!(prompt);
    }
    let result = client
        .post(&format!("/api/scout/items/{id}/act"), &body)
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_publish(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .post(&format!("/api/scout/items/{id}/telegraph"), &json!({}))
        .await?;
    let url = result["url"].as_str().unwrap_or("(missing url)");
    println!("{url}");
    Ok(())
}

async fn handle_sessions(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get(&format!("/api/scout/items/{id}/sessions"))
        .await?;
    if let Some(sessions) = result.as_array() {
        if sessions.is_empty() {
            println!("No sessions linked to scout item #{id}.");
            return Ok(());
        }
        println!("Scout item #{id} sessions");
        println!("{}", "-".repeat(60));
        for session in sessions {
            let session_id = session["session_id"].as_str().unwrap_or("?");
            let caller = session["caller"].as_str().unwrap_or("?");
            let started = session["created_at"].as_str().unwrap_or("?");
            let status = session["status"].as_str().unwrap_or("?");
            println!("{session_id:<38}  {caller:<12}  {status:<10}  {started}");
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?);
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
