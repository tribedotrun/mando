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
    /// Update item status
    Status {
        /// Item ID
        id: i64,
        /// New status
        status: String,
    },
    /// Process pending items
    Process {
        /// Specific item ID (omit for all pending)
        #[arg(long)]
        id: Option<i64>,
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
}

pub(crate) async fn handle(args: ScoutArgs) -> anyhow::Result<()> {
    match args.command {
        ScoutCommand::SimpleList { status } => handle_list(status.as_deref()).await,
        ScoutCommand::Add { url, title } => handle_add(&url, title.as_deref()).await,
        ScoutCommand::Show { id } => handle_show(id).await,
        ScoutCommand::Delete { id } => handle_delete(id).await,
        ScoutCommand::Status { id, status } => handle_status(id, &status).await,
        ScoutCommand::Process { id } => handle_process(id).await,
        ScoutCommand::List { status } => handle_list_with_summaries(status.as_deref()).await,
        ScoutCommand::Save { id } => handle_status(id, "saved").await,
        ScoutCommand::Archive { id } => handle_status(id, "archived").await,
        ScoutCommand::Read { id } => handle_read(id).await,
        ScoutCommand::Ask { id, question } => handle_ask(id, &question.join(" ")).await,
        ScoutCommand::Research { topic, process } => {
            handle_research(&topic.join(" "), process).await
        }
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

async fn handle_status(id: i64, status: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"status": status});
    client
        .patch(&format!("/api/scout/items/{id}"), &body)
        .await?;
    println!("Updated item #{id} status to '{status}'.");
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
                (Some(r), Some(q)) => format!(" R:{r}\u{00b7}Q:{q}"),
                _ => String::new(),
            };
            println!("#{id} [{st}] {title}{scores}");

            // Fetch full item to get summary.
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
    Ok(())
}

async fn handle_ask(id: i64, question: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": id, "question": question});
    let result = client.post("/api/scout/ask", &body).await?;

    if let Some(answer) = result["answer"].as_str() {
        println!("{answer}");
    } else if result["ok"].as_bool() == Some(true) {
        println!("(No answer returned)");
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

async fn handle_process(id: Option<i64>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({});
    if let Some(i) = id {
        body["id"] = json!(i);
    }
    let result = client.post("/api/scout/process", &body).await?;
    let processed = result["processed"].as_u64().unwrap_or(0);
    println!("Processed {processed} item(s).");
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
    fn parse_scout_show() {
        let cli = TestCli::try_parse_from(["test", "scout", "show", "42"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Show { id } => assert_eq!(id, 42),
                _ => panic!("expected Show"),
            },
        }
    }

    #[test]
    fn parse_scout_delete() {
        let cli = TestCli::try_parse_from(["test", "scout", "delete", "7"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Delete { id } => assert_eq!(id, 7),
                _ => panic!("expected Delete"),
            },
        }
    }

    #[test]
    fn parse_scout_process() {
        let cli = TestCli::try_parse_from(["test", "scout", "process"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Process { id } => assert!(id.is_none()),
                _ => panic!("expected Process"),
            },
        }
    }

    #[test]
    fn parse_scout_list() {
        let cli = TestCli::try_parse_from(["test", "scout", "list"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::List { status } => assert!(status.is_none()),
                _ => panic!("expected List"),
            },
        }
    }

    #[test]
    fn parse_scout_save() {
        let cli = TestCli::try_parse_from(["test", "scout", "save", "5"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Save { id } => assert_eq!(id, 5),
                _ => panic!("expected Save"),
            },
        }
    }

    #[test]
    fn parse_scout_archive() {
        let cli = TestCli::try_parse_from(["test", "scout", "archive", "8"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Archive { id } => assert_eq!(id, 8),
                _ => panic!("expected Archive"),
            },
        }
    }

    #[test]
    fn parse_scout_read() {
        let cli = TestCli::try_parse_from(["test", "scout", "read", "3"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Read { id } => assert_eq!(id, 3),
                _ => panic!("expected Read"),
            },
        }
    }

    #[test]
    fn parse_scout_ask() {
        let cli = TestCli::try_parse_from([
            "test", "scout", "ask", "42", "What", "is", "the", "main", "point?",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Ask { id, question } => {
                    assert_eq!(id, 42);
                    assert_eq!(question.join(" "), "What is the main point?");
                }
                _ => panic!("expected Ask"),
            },
        }
    }

    #[test]
    fn parse_scout_research() {
        let cli =
            TestCli::try_parse_from(["test", "scout", "research", "AI", "agents", "--process"])
                .unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::Research { topic, process } => {
                    assert_eq!(topic.join(" "), "AI agents");
                    assert!(process);
                }
                _ => panic!("expected Research"),
            },
        }
    }

    #[test]
    fn parse_scout_list_status() {
        let cli = TestCli::try_parse_from(["test", "scout", "list", "--status", "saved"]).unwrap();
        match cli.cmd {
            TestCmd::Scout(args) => match args.command {
                ScoutCommand::List { status } => {
                    assert_eq!(status.as_deref(), Some("saved"));
                }
                _ => panic!("expected List"),
            },
        }
    }
}
