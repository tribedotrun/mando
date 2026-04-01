//! mando CLI — thin HTTP client for the mando-gw daemon.
//!
//! All domain operations are proxied through HTTP to the running daemon.
//! Only worktree management runs locally (git commands).

mod captain;
mod captain_review;
mod cron;
mod gateway;
mod http;
mod ops;
mod project;
mod scout;
mod sessions;
mod todo;
mod voice;
mod worktree;

use clap::{Args, Parser, Subcommand};
use serde_json::json;

use crate::http::DaemonClient;

#[derive(Parser)]
#[command(name = "mando", about = "Mando — AI-powered development automation")]
struct Cli {
    /// Enable verbose logging (DEBUG level)
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage tasks
    Todo(todo::TodoArgs),
    /// Manage configured projects
    Project(project::ProjectArgs),
    /// Captain tick loop and worker management
    Captain(captain::CaptainArgs),
    /// Cron job management
    Cron(cron::CronArgs),
    /// Multi-turn ops copilot
    Ops(ops::OpsArgs),
    /// Scout management
    Scout(scout::ScoutArgs),
    /// CC session history
    Sessions(sessions::SessionsArgs),
    /// Voice control and TTS usage
    Voice(voice::VoiceArgs),
    /// Git worktree management
    Worktree(worktree::WorktreeArgs),
    /// Daemon lifecycle management
    Daemon(gateway::DaemonArgs),
    /// Show configured channels
    Channels(ChannelsArgs),
    /// Squash-merge a PR (alias for captain merge)
    Merge(MergeArgs),
    /// Send a notification via Telegram
    Notify(NotifyArgs),
    /// Firecrawl web scraping
    Firecrawl(FirecrawlArgs),
    /// Triage pending-review tasks
    Triage(TriageArgs),
    /// Show task overview grouped by repo and workflow state
    Tasks(TasksArgs),
    /// Show system health (daemon, workers, config)
    Health(HealthArgs),
}

// -----------------------------------------------------------------------
// Simple top-level commands (no subcommands)
// -----------------------------------------------------------------------

#[derive(Args)]
struct ChannelsArgs;

#[derive(Args)]
struct MergeArgs {
    /// PR number
    pr_num: String,
    /// Project name
    #[arg(short = 'p', long = "project")]
    project: Option<String>,
}

#[derive(Args)]
struct NotifyArgs {
    /// Message to send
    message: String,
    /// Chat ID (defaults to configured telegram.owner)
    #[arg(long)]
    chat_id: Option<String>,
}

#[derive(Args)]
struct FirecrawlArgs {
    #[command(subcommand)]
    command: FirecrawlCommand,
}

#[derive(Subcommand)]
enum FirecrawlCommand {
    /// Scrape a URL and output markdown
    Scrape { url: String },
}

#[derive(Args)]
struct TriageArgs {
    /// Specific task ID to triage
    item_id: Option<String>,
}

#[derive(Args)]
struct TasksArgs {
    /// Include archived/merged items
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
struct HealthArgs;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("mando={level}").parse().unwrap()),
        )
        .init();

    let result = match cli.command {
        Commands::Todo(args) => todo::handle(args).await,
        Commands::Project(args) => project::handle(args).await,
        Commands::Captain(args) => captain::handle(args).await,
        Commands::Cron(args) => cron::handle(args).await,
        Commands::Ops(args) => ops::handle(args).await,
        Commands::Scout(args) => scout::handle(args).await,
        Commands::Sessions(args) => sessions::handle(args).await,
        Commands::Voice(args) => voice::handle(args).await,
        Commands::Worktree(args) => worktree::handle(args).await,
        Commands::Daemon(args) => gateway::handle(args).await,
        Commands::Channels(_) => handle_channels().await,
        Commands::Merge(args) => handle_merge(args).await,
        Commands::Notify(args) => handle_notify(args).await,
        Commands::Firecrawl(args) => handle_firecrawl(args).await,
        Commands::Triage(args) => handle_triage(args).await,
        Commands::Tasks(args) => handle_tasks(args).await,
        Commands::Health(_) => handle_health().await,
    };

    if let Err(e) = result {
        let msg = format!("{e:#}");
        if msg.contains("Connection refused") || msg.contains("tcp connect error") {
            eprintln!(
                "error: daemon not running (connection refused). Start with: mando daemon start"
            );
        } else if msg.contains("daemon not running") {
            eprintln!("error: daemon not running. Start with: mando daemon start");
        } else {
            eprintln!("error: {msg}");
        }
        std::process::exit(1);
    }
}

// -----------------------------------------------------------------------
// Top-level command handlers (all via HTTP)
// -----------------------------------------------------------------------

async fn handle_channels() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/channels").await?;
    println!("Channels:");
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_merge(args: MergeArgs) -> anyhow::Result<()> {
    captain::handle_merge_pr(&args.pr_num, args.project.as_deref()).await
}

async fn handle_notify(args: NotifyArgs) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"message": args.message});
    if let Some(chat_id) = args.chat_id {
        body["chat_id"] = json!(chat_id);
    }
    client.post("/api/notify", &body).await?;
    println!("Notification sent.");
    Ok(())
}

async fn handle_firecrawl(args: FirecrawlArgs) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    match args.command {
        FirecrawlCommand::Scrape { url } => {
            let body = json!({"url": url});
            let result = client.post("/api/firecrawl/scrape", &body).await?;
            let content = result["content"].as_str().unwrap_or("");
            println!("{content}");
        }
    }
    Ok(())
}

async fn handle_tasks(args: TasksArgs) -> anyhow::Result<()> {
    use std::collections::{BTreeMap, HashMap};

    let client = DaemonClient::discover()?;

    let api_path = if args.all {
        "/api/tasks?include_archived=true"
    } else {
        "/api/tasks"
    };

    let result = client.get(api_path).await?;
    let items = match result["items"].as_array() {
        Some(arr) if !arr.is_empty() => arr,
        _ => {
            println!("No tasks.");
            return Ok(());
        }
    };

    // Status display order (matches Telegram).
    const STATUS_ORDER: &[&str] = &[
        "new",
        "clarifying",
        "needs-clarification",
        "queued",
        "in-progress",
        "captain-reviewing",
        "captain-merging",
        "awaiting-review",
        "handed-off",
        "rework",
        "escalated",
        "errored",
        "merged",
        "completed-no-pr",
        "canceled",
    ];

    // Count by status.
    let mut status_counts: HashMap<&str, usize> = HashMap::new();
    for item in items {
        let s = item["status"].as_str().unwrap_or("unknown");
        *status_counts.entry(s).or_default() += 1;
    }
    let summary: Vec<String> = STATUS_ORDER
        .iter()
        .filter_map(|s| status_counts.get(s).map(|c| format!("{s}={c}")))
        .collect();

    println!("Tasks ({} items)", items.len());
    println!("{}", summary.join(" "));

    // Group by project.
    let mut by_project: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for item in items {
        let project = item["project"].as_str().unwrap_or("unknown").to_string();
        by_project.entry(project).or_default().push(item);
    }

    for (project, project_items) in &by_project {
        println!("\n  {project}");

        for &status in STATUS_ORDER {
            let status_items: Vec<_> = project_items
                .iter()
                .filter(|it| it["status"].as_str() == Some(status))
                .collect();
            if status_items.is_empty() {
                continue;
            }

            println!("    {status} ({})", status_items.len());

            for item in &status_items {
                let id = item["id"].as_i64().unwrap_or(0);
                let linear_id = item["linear_id"].as_str().unwrap_or("");
                let title = item["title"].as_str().unwrap_or("?");
                let worker = item["worker"].as_str().unwrap_or("");
                let pr = item["pr"].as_str().unwrap_or("");

                let id_str = if !linear_id.is_empty() {
                    linear_id.to_string()
                } else {
                    format!("#{id}")
                };

                let mut suffix = String::new();
                if !worker.is_empty() {
                    suffix.push_str(&format!(" | {worker}"));
                }
                if !pr.is_empty() {
                    let num = pr.rsplit('/').next().unwrap_or(pr).trim_start_matches('#');
                    suffix.push_str(&format!(" | PR #{num}"));
                }

                println!("      {id_str} {title}{suffix}");
            }
        }
    }

    Ok(())
}

async fn handle_health() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let health = client.get("/api/health/system").await?;
    let version = health["version"].as_str().unwrap_or("?");
    let pid = health["pid"].as_u64().unwrap_or(0);
    let uptime = health["uptime"].as_u64().unwrap_or(0);
    let active = health["active_workers"].as_u64().unwrap_or(0);
    let total = health["total_items"].as_u64().unwrap_or(0);
    let config_path = health["configPath"].as_str().unwrap_or("?");
    let data_dir = health["dataDir"].as_str().unwrap_or("?");
    let projects = health["projects"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    println!("Mando Health");
    println!("{}", "-".repeat(40));
    println!("  Daemon:         v{version} (pid {pid}, uptime {uptime}s)");
    println!("  Active workers: {active}");
    println!("  Tasks:          {total}");
    println!("  Projects:       {projects}");
    println!("  Config:         {config_path}");
    println!("  Data dir:       {data_dir}");

    Ok(())
}

async fn handle_triage(args: TriageArgs) -> anyhow::Result<()> {
    captain::handle_triage_cmd(args.item_id.as_deref()).await
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use serde_json::Value;

    #[test]
    fn cli_parse_todo_list() {
        let cli = Cli::try_parse_from(["mando", "todo", "list"]).unwrap();
        assert!(matches!(cli.command, Commands::Todo(_)));
    }

    #[test]
    fn cli_parse_todo_list_all() {
        let cli = Cli::try_parse_from(["mando", "todo", "list", "--all"]).unwrap();
        assert!(matches!(cli.command, Commands::Todo(_)));
    }

    #[test]
    fn cli_parse_captain_tick() {
        let cli = Cli::try_parse_from(["mando", "captain", "tick", "--dry-run"]).unwrap();
        assert!(matches!(cli.command, Commands::Captain(_)));
    }

    #[test]
    fn cli_parse_cron_list() {
        let cli = Cli::try_parse_from(["mando", "cron", "list"]).unwrap();
        assert!(matches!(cli.command, Commands::Cron(_)));
    }

    #[test]
    fn cli_parse_daemon_start() {
        let cli = Cli::try_parse_from(["mando", "daemon", "start", "-p", "9999"]).unwrap();
        assert!(matches!(cli.command, Commands::Daemon(_)));
    }

    #[test]
    fn cli_parse_daemon_stop() {
        let cli = Cli::try_parse_from(["mando", "daemon", "stop"]).unwrap();
        assert!(matches!(cli.command, Commands::Daemon(_)));
    }

    #[test]
    fn cli_parse_daemon_health() {
        let cli = Cli::try_parse_from(["mando", "daemon", "health"]).unwrap();
        assert!(matches!(cli.command, Commands::Daemon(_)));
    }

    #[test]
    fn cli_parse_merge() {
        let cli = Cli::try_parse_from(["mando", "merge", "123", "-p", "mando"]).unwrap();
        assert!(matches!(cli.command, Commands::Merge(_)));
    }

    #[test]
    fn cli_parse_sessions() {
        let cli = Cli::try_parse_from(["mando", "sessions"]).unwrap();
        assert!(matches!(cli.command, Commands::Sessions(_)));
    }

    #[test]
    fn cli_parse_worktree_list() {
        let cli = Cli::try_parse_from(["mando", "worktree", "list"]).unwrap();
        assert!(matches!(cli.command, Commands::Worktree(_)));
    }

    #[test]
    fn cli_parse_scout_list() {
        let cli = Cli::try_parse_from(["mando", "scout", "list"]).unwrap();
        assert!(matches!(cli.command, Commands::Scout(_)));
    }

    #[test]
    fn cli_parse_channels() {
        let cli = Cli::try_parse_from(["mando", "channels"]).unwrap();
        assert!(matches!(cli.command, Commands::Channels(_)));
    }

    #[test]
    fn cli_parse_tasks() {
        let cli = Cli::try_parse_from(["mando", "tasks"]).unwrap();
        assert!(matches!(cli.command, Commands::Tasks(_)));
    }

    #[test]
    fn cli_parse_health() {
        let cli = Cli::try_parse_from(["mando", "health"]).unwrap();
        assert!(matches!(cli.command, Commands::Health(_)));
    }

    #[test]
    fn capability_contract_matches_cli_captain_and_scout_surfaces() {
        let contract: Value =
            serde_json::from_str(include_str!("../../contracts/capabilities.json")).unwrap();

        let root = Cli::command();
        let root_names: std::collections::HashSet<String> = root
            .get_subcommands()
            .map(|subcommand: &clap::Command| subcommand.get_name().to_string())
            .collect();
        assert!(
            contract["captain"].get("tasks").is_some(),
            "missing captain tasks in contract"
        );
        assert!(
            root_names.contains("tasks"),
            "missing top-level tasks command"
        );

        let captain = root.find_subcommand("captain").unwrap();
        let captain_names: std::collections::HashSet<String> = captain
            .get_subcommands()
            .map(|subcommand: &clap::Command| subcommand.get_name().to_string())
            .collect();
        for (expected, command_name) in [
            ("workers", "workers"),
            ("triage", "triage"),
            ("reopen", "reopen"),
            ("rework", "rework"),
            ("retry", "retry"),
            ("accept", "accept"),
            ("handoff", "handoff"),
            ("adopt", "adopt"),
            ("nudge", "nudge"),
            ("stop", "stop"),
            ("knowledge_review", "knowledge"),
            ("journal", "journal"),
            ("patterns", "patterns"),
        ] {
            assert!(
                contract["captain"].get(expected).is_some(),
                "missing {expected} in contract"
            );
            assert!(
                captain_names.contains(command_name),
                "missing captain {command_name}"
            );
        }

        let scout = root.find_subcommand("scout").unwrap();
        let scout_names: std::collections::HashSet<String> = scout
            .get_subcommands()
            .map(|subcommand: &clap::Command| subcommand.get_name().to_string())
            .collect();
        for (expected, command_name) in [
            ("add", "add"),
            ("research", "research"),
            ("read", "read"),
            ("ask", "ask"),
            ("act", "act"),
            ("save", "save"),
            ("archive", "archive"),
            ("delete", "delete"),
            ("bulk_update", "bulk-status"),
            ("bulk_delete", "bulk-delete"),
            ("publish_article", "publish"),
            ("item_sessions", "sessions"),
        ] {
            assert!(
                contract["scout"].get(expected).is_some(),
                "missing {expected} in contract"
            );
            assert!(
                scout_names.contains(command_name),
                "missing scout {command_name}"
            );
        }
    }
}
