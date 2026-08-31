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
    let result = client.get_scout_items(&scout_query(status)?).await?;

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
    let result = client
        .post_scout_items(&api_types::ScoutAddRequest {
            url: url.to_string(),
            title: title.map(str::to_string),
        })
        .await?;
    println!("Added scout item #{}: {url}", result.id);
    Ok(())
}

async fn handle_show(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
        .await?;

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

fn status_filter(status: Option<&str>) -> anyhow::Result<Option<api_types::ScoutItemStatusFilter>> {
    let Some(status) = status else {
        return Ok(None);
    };
    let value = match status {
        "all" => api_types::ScoutItemStatusFilter::All,
        "pending" => api_types::ScoutItemStatusFilter::Pending,
        "fetched" => api_types::ScoutItemStatusFilter::Fetched,
        "processed" => api_types::ScoutItemStatusFilter::Processed,
        "saved" => api_types::ScoutItemStatusFilter::Saved,
        "archived" => api_types::ScoutItemStatusFilter::Archived,
        "error" => api_types::ScoutItemStatusFilter::Error,
        other => anyhow::bail!(
            "unsupported scout status filter '{other}' (use all, pending, fetched, processed, saved, archived, error)"
        ),
    };
    Ok(Some(value))
}

fn scout_query(status: Option<&str>) -> anyhow::Result<api_types::ScoutQuery> {
    Ok(api_types::ScoutQuery {
        status: status_filter(status)?,
        q: None,
        item_type: None,
        page: None,
        per_page: Some(10_000),
    })
}

async fn handle_delete(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .delete_scout_items_by_id(&api_types::ScoutItemIdParams { id })
        .await?;
    println!("Deleted scout item #{id}.");
    Ok(())
}

async fn handle_bulk_delete(ids: &[i64]) -> anyhow::Result<()> {
    if ids.is_empty() {
        anyhow::bail!("provide at least one item ID");
    }
    let client = DaemonClient::discover()?;
    let result = client
        .post_scout_bulkdelete(&api_types::ScoutBulkDeleteRequest { ids: ids.to_vec() })
        .await?;
    println!("Deleted {} scout item(s).", result.deleted);
    Ok(())
}

async fn handle_status(id: i64, status: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let command = lifecycle_command_for_status(status)?;
    client
        .patch_scout_items_by_id(
            &api_types::ScoutItemIdParams { id },
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
    let result = client
        .post_scout_bulk(&api_types::ScoutBulkCommandRequest {
            ids: ids.to_vec(),
            command,
        })
        .await?;
    println!("Updated {} scout item(s) to '{status}'.", result.updated);
    Ok(())
}

async fn handle_list_with_summaries(status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get_scout_items(&scout_query(status)?).await?;

    for item in &result.items {
        let scores = match (item.relevance, item.quality) {
            (Some(r), Some(q)) => format!(" R:{r}·Q:{q}"),
            _ => String::new(),
        };
        let title = item.title.as_deref().unwrap_or(item.url.as_str());
        println!("#{} [{}] {title}{scores}", item.id, item.status);

        if let Some(summary) = item.summary.as_deref() {
            for line in summary.lines().take(3) {
                println!("  {line}");
            }
        }
        println!();
    }

    println!("{} item(s)", result.total);
    Ok(())
}

async fn handle_read(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get_scout_items_by_id_article(&api_types::ScoutItemIdParams { id })
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
    let result = client
        .post_scout_ask(&api_types::ScoutAskRequest {
            id,
            question: question.to_string(),
            session_id: session.map(str::to_string),
        })
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
    let result = client
        .post_scout_research(&api_types::ScoutResearchRequest {
            topic: topic.to_string(),
            process: Some(true),
        })
        .await?;
    let run_id = result.run_id;
    println!("Research started (run #{run_id})\n");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let run = client
            .get_scout_research_by_id(&api_types::ScoutResearchIdParams { id: run_id })
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
    let result = client
        .post_scout_items_by_id_act(
            &api_types::ScoutItemIdParams { id },
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
    let result = client
        .post_scout_items_by_id_telegraph(&api_types::ScoutItemIdParams { id })
        .await?;
    println!("{}", result.url);
    Ok(())
}

async fn handle_sessions(id: i64) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .get_scout_items_by_id_sessions(&api_types::ScoutItemIdParams { id })
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
