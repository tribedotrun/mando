//! mando CLI — thin HTTP client for the mando-gw daemon.
//!
//! All domain operations are proxied through HTTP to the running daemon.
//! Local repository inspection uses the shared global-git provider boundary.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI stdout and stderr are the user-facing shell and JSON protocol"
)]

mod captain;
mod codex_app;
mod credentials;
mod credentials_codex_pick;
mod gateway;
mod http;
mod motion_check;
mod project;
mod scout;
mod sessions;
mod todo;
mod todo_artifacts;
mod todo_display;
mod transcript_render;
mod worktree;

#[cfg(test)]
mod cli_contract_tests;

use clap::{Args, Parser, Subcommand};

use crate::http::{daemon_friendly_message, DaemonClient};

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
    /// Scout management
    Scout(scout::ScoutArgs),
    /// CC session history
    Sessions(sessions::SessionsArgs),
    /// Git worktree management
    Worktree(worktree::WorktreeArgs),
    /// Daemon lifecycle management
    Daemon(gateway::DaemonArgs),
    /// Launch the Electron UI
    Ui(UiArgs),
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
    /// Manage and inspect Claude credentials (multi-account pool)
    Credentials(credentials::CredentialsArgs),
    /// Codex desktop-app account swap (ChatGPT app shared auth.json slot)
    Codex(codex_app::CodexArgs),
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

#[derive(Args)]
struct UiArgs;

fn task_status_label(status: api_types::ItemStatus) -> &'static str {
    match status {
        api_types::ItemStatus::New => "new",
        api_types::ItemStatus::Clarifying => "clarifying",
        api_types::ItemStatus::NeedsClarification => "needs-clarification",
        api_types::ItemStatus::Queued => "queued",
        api_types::ItemStatus::InProgress => "in-progress",
        api_types::ItemStatus::CaptainReviewing => "captain-reviewing",
        api_types::ItemStatus::CaptainMerging => "captain-merging",
        api_types::ItemStatus::AwaitingReview => "awaiting-review",
        api_types::ItemStatus::Rework => "rework",
        api_types::ItemStatus::HandedOff => "handed-off",
        api_types::ItemStatus::Escalated => "escalated",
        api_types::ItemStatus::Errored => "errored",
        api_types::ItemStatus::Merged => "merged",
        api_types::ItemStatus::CompletedNoPr => "completed-no-pr",
        api_types::ItemStatus::Canceled => "canceled",
        api_types::ItemStatus::Stopped => "stopped",
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let level = if cli.verbose { "debug" } else { "info" };
    let directive = match format!("mando={level}").parse() {
        Ok(d) => d,
        Err(e) => global_infra::unrecoverable!("invalid tracing directive", e),
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(directive))
        .init();

    let result = match cli.command {
        Commands::Todo(args) => todo::handle(args).await,
        Commands::Project(args) => project::handle(args).await,
        Commands::Captain(args) => captain::handle(args).await,
        Commands::Scout(args) => scout::handle(args).await,
        Commands::Sessions(args) => sessions::handle(args).await,
        Commands::Worktree(args) => worktree::handle(args).await,
        Commands::Daemon(args) => gateway::handle(args).await,
        Commands::Ui(_) => handle_ui_launch().await,
        Commands::Channels(_) => handle_channels().await,
        Commands::Merge(args) => handle_merge(args).await,
        Commands::Notify(args) => handle_notify(args).await,
        Commands::Firecrawl(args) => handle_firecrawl(args).await,
        Commands::Triage(args) => handle_triage(args).await,
        Commands::Tasks(args) => handle_tasks(args).await,
        Commands::Health(_) => handle_health().await,
        Commands::Credentials(args) => credentials::handle(args).await,
        Commands::Codex(args) => codex_app::handle(args).await,
    };

    if let Err(e) = result {
        if let Some(message) = daemon_friendly_message(&e) {
            eprintln!("{message}");
        } else {
            eprintln!("error: {e:#}");
        }
        std::process::exit(1);
    }
}

// -----------------------------------------------------------------------
// Top-level command handlers (all via HTTP)
// -----------------------------------------------------------------------

async fn handle_channels() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get_channels().await?;
    println!("Channels:");
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_merge(args: MergeArgs) -> anyhow::Result<()> {
    captain::handle_merge_pr(&args.pr_num, args.project.as_deref()).await
}

async fn handle_notify(args: NotifyArgs) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_notify(&api_types::NotifyRequest {
            message: args.message,
            chat_id: args.chat_id,
        })
        .await?;
    println!("Notification sent.");
    Ok(())
}

async fn handle_firecrawl(args: FirecrawlArgs) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    match args.command {
        FirecrawlCommand::Scrape { url } => {
            let result = client
                .post_firecrawl_scrape(&api_types::FirecrawlScrapeRequest { url })
                .await?;
            println!("{}", result.content);
        }
    }
    Ok(())
}

async fn handle_tasks(args: TasksArgs) -> anyhow::Result<()> {
    use std::collections::{BTreeMap, HashMap};

    let client = DaemonClient::discover()?;

    let result = client
        .get_tasks(&api_types::TaskListQuery {
            include_archived: args.all.then_some(true),
        })
        .await?;
    if result.items.is_empty() {
        println!("No tasks.");
        return Ok(());
    }
    let items = result.items;

    // Status display order (matches Telegram).
    const STATUS_ORDER: &[api_types::ItemStatus] = &[
        api_types::ItemStatus::New,
        api_types::ItemStatus::Clarifying,
        api_types::ItemStatus::NeedsClarification,
        api_types::ItemStatus::Queued,
        api_types::ItemStatus::InProgress,
        api_types::ItemStatus::CaptainReviewing,
        api_types::ItemStatus::CaptainMerging,
        api_types::ItemStatus::AwaitingReview,
        api_types::ItemStatus::HandedOff,
        api_types::ItemStatus::Rework,
        api_types::ItemStatus::Escalated,
        api_types::ItemStatus::Errored,
        api_types::ItemStatus::Stopped,
        api_types::ItemStatus::Merged,
        api_types::ItemStatus::CompletedNoPr,
        api_types::ItemStatus::Canceled,
    ];

    // Count by status.
    let mut status_counts: HashMap<&'static str, usize> = HashMap::new();
    for item in &items {
        *status_counts
            .entry(task_status_label(item.status))
            .or_default() += 1;
    }
    let summary: Vec<String> = STATUS_ORDER
        .iter()
        .filter_map(|s| {
            status_counts
                .get(task_status_label(*s))
                .map(|c| format!("{}={c}", task_status_label(*s)))
        })
        .collect();

    println!("Tasks ({} items)", items.len());
    println!("{}", summary.join(" "));

    // Group by project.
    let mut by_project: BTreeMap<String, Vec<&api_types::TaskItem>> = BTreeMap::new();
    for item in &items {
        let project = item.project.clone().unwrap_or_else(|| "unknown".into());
        by_project.entry(project).or_default().push(item);
    }

    for (project, project_items) in &by_project {
        println!("\n  {project}");

        for status in STATUS_ORDER {
            let status_items: Vec<_> = project_items
                .iter()
                .filter(|it| task_status_label(it.status) == task_status_label(*status))
                .collect();
            if status_items.is_empty() {
                continue;
            }

            println!(
                "    {} ({})",
                task_status_label(*status),
                status_items.len()
            );

            for item in &status_items {
                let id = item.id;
                let title = item.title.as_str();
                let worker = item.worker.as_deref().unwrap_or("");
                let pr = item.pr_number.map(|n| format!("#{n}")).unwrap_or_default();

                let id_str = format!("#{id}");

                let mut suffix = String::new();
                if !worker.is_empty() {
                    suffix.push_str(&format!(" | {worker}"));
                }
                if !pr.is_empty() {
                    suffix.push_str(&format!(" | PR {pr}"));
                }

                println!("      {id_str} {title}{suffix}");
            }
        }
    }

    Ok(())
}

async fn handle_health() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let health = client
        .accepting_server_error_bodies()
        .get_health_system()
        .await?;
    let version = health.version;
    let pid = health.pid;
    let uptime = health.uptime;
    let active = health.active_workers;
    let total = health.total_items;
    let config_path = health.config_path;
    let data_dir = health.data_dir;
    let projects = health.projects.join(", ");

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

async fn handle_ui_launch() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client.post_ui_launch().await?;
    println!("UI launch requested");
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
