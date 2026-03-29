//! `mando captain` — tick loop and worker management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct CaptainArgs {
    #[command(subcommand)]
    pub command: CaptainCommand,
}

#[derive(Subcommand)]
pub(crate) enum CaptainCommand {
    /// Run one captain tick cycle
    Tick {
        /// Dry-run mode (no mutations)
        #[arg(long)]
        dry_run: bool,
        /// Telegram chat ID for notifications
        #[arg(long)]
        notify_chat_id: Option<String>,
    },
    /// Show active workers table
    Workers {
        /// Watch mode (auto-refresh)
        #[arg(short = 'w')]
        watch: bool,
        /// Refresh interval in seconds (default 5)
        #[arg(short = 'n')]
        interval: Option<u64>,
    },
    /// Squash-merge a PR
    Merge {
        /// PR number
        pr_num: String,
        /// Project name
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
    },
    /// AI-scored triage of awaiting-review items by merge-readiness
    Triage {
        /// Optional specific task ID to triage
        item_id: Option<String>,
    },
    /// Reopen a completed/failed item with feedback
    Reopen {
        /// Item ID
        id: String,
        /// Feedback for the worker
        feedback: String,
    },
    /// Rework an item (fresh worktree, new worker)
    Rework {
        /// Item ID
        id: String,
        /// Feedback/instructions
        feedback: String,
    },
    /// Retry an errored item (re-trigger captain review)
    Retry {
        /// Item ID
        id: String,
    },
    /// Hand off an item to human (worker -> human)
    Handoff {
        /// Item ID
        id: String,
    },
    /// Adopt a human's in-progress worktree (captain takes over)
    Adopt {
        /// Task title
        title: String,
        /// Worktree path (defaults to current directory)
        #[arg(short = 'w', long)]
        worktree: Option<String>,
        /// Note/instructions for the worker
        #[arg(short = 'n', long)]
        note: Option<String>,
        /// Project name
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
    },
    /// Nudge a stuck worker with a message
    Nudge {
        /// Item ID
        id: String,
        /// Nudge message to deliver to the worker
        message: String,
    },
    /// Graceful stop (kill all workers, drain tasks)
    Stop,
    /// Approve/review knowledge lessons
    Knowledge,
    /// Query the decision journal
    Journal {
        /// Filter by worker name
        #[arg(short = 'w', long)]
        worker: Option<String>,
        /// Number of entries to show (default 20)
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
    /// Run the pattern distiller or view patterns
    Patterns {
        /// Show existing patterns instead of running distiller
        #[arg(long)]
        list: bool,
        /// Filter patterns by status (pending/approved/dismissed)
        #[arg(long)]
        status: Option<String>,
    },
}

pub(crate) async fn handle(args: CaptainArgs) -> anyhow::Result<()> {
    match args.command {
        CaptainCommand::Tick {
            dry_run,
            notify_chat_id,
        } => handle_tick(dry_run, notify_chat_id).await,
        CaptainCommand::Workers { watch, interval } => handle_workers(watch, interval).await,
        CaptainCommand::Merge { pr_num, project } => {
            handle_merge_pr(&pr_num, project.as_deref()).await
        }
        CaptainCommand::Triage { item_id } => handle_triage(item_id.as_deref()).await,
        CaptainCommand::Reopen { id, feedback } => handle_reopen(&id, &feedback).await,
        CaptainCommand::Rework { id, feedback } => handle_rework(&id, &feedback).await,
        CaptainCommand::Retry { id } => handle_retry(&id).await,
        CaptainCommand::Adopt {
            title,
            worktree,
            note,
            project,
        } => {
            handle_adopt(
                &title,
                worktree.as_deref(),
                note.as_deref(),
                project.as_deref(),
            )
            .await
        }
        CaptainCommand::Handoff { id } => handle_handoff(&id).await,
        CaptainCommand::Nudge { id, message } => handle_nudge(&id, &message).await,
        CaptainCommand::Stop => handle_captain_stop().await,
        CaptainCommand::Knowledge => handle_knowledge().await,
        CaptainCommand::Journal { worker, limit } => {
            handle_journal(worker.as_deref(), limit.unwrap_or(20)).await
        }
        CaptainCommand::Patterns { list, status } => handle_patterns(list, status.as_deref()).await,
    }
}

async fn handle_tick(dry_run: bool, notify_chat_id: Option<String>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"dry_run": dry_run});
    if let Some(chat_id) = notify_chat_id {
        body["notify_chat_id"] = json!(chat_id);
    }
    let result = client.post("/api/captain/tick", &body).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_workers(watch: bool, interval: Option<u64>) -> anyhow::Result<()> {
    let interval_secs = interval.unwrap_or(5);
    let client = DaemonClient::discover()?;
    loop {
        let health = client.get("/api/health/system").await?;

        if watch {
            // Clear screen for watch mode.
            print!("\x1b[2J\x1b[H");
        }

        let active = health["active_workers"].as_u64().unwrap_or(0);
        let total = health["total_items"].as_u64().unwrap_or(0);
        let projects = health["projects"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        println!("Captain Workers");
        println!("{}", "-".repeat(40));
        println!("  Active workers: {active}");
        println!("  Tasks:          {total}");
        println!("  Projects:       {projects}");

        if !watch {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
    Ok(())
}

async fn handle_triage(item_id: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({});
    if let Some(id) = item_id {
        body["item_id"] = json!(id);
    }
    let result = client.post("/api/captain/triage", &body).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_reopen(id: &str, feedback: &str) -> anyhow::Result<()> {
    let id_num: i64 = id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid item ID: {id}"))?;
    let client = DaemonClient::discover()?;
    let body = json!({"id": id_num, "feedback": feedback});
    client.post("/api/tasks/reopen", &body).await?;
    println!("Reopened item {id}");
    Ok(())
}

async fn handle_rework(id: &str, feedback: &str) -> anyhow::Result<()> {
    let id_num: i64 = id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid item ID: {id}"))?;
    let client = DaemonClient::discover()?;
    let body = json!({"id": id_num, "feedback": feedback});
    client.post("/api/tasks/rework", &body).await?;
    println!("Rework requested for item {id}");
    Ok(())
}

async fn handle_retry(id: &str) -> anyhow::Result<()> {
    let id_num: i64 = id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid item ID: {id}"))?;
    let client = DaemonClient::discover()?;
    let body = json!({"id": id_num});
    client.post("/api/tasks/retry", &body).await?;
    println!("Retried item {id} — re-entering captain review");
    Ok(())
}

async fn handle_handoff(id: &str) -> anyhow::Result<()> {
    let id_num: i64 = id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid item ID: {id}"))?;
    let client = DaemonClient::discover()?;
    let body = json!({"id": id_num});
    client.post("/api/tasks/handoff", &body).await?;
    println!("Handed off item {id} to human");
    Ok(())
}

async fn handle_adopt(
    title: &str,
    worktree: Option<&str>,
    note: Option<&str>,
    project: Option<&str>,
) -> anyhow::Result<()> {
    let wt_path = match worktree {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir()?,
    };

    // Validate worktree.
    if !wt_path.join(".git").exists() {
        anyhow::bail!("not a git worktree: {}", wt_path.display());
    }

    // Detect branch.
    let mut branch = String::new();
    for args in [
        ["branch", "--show-current"].as_slice(),
        ["rev-parse", "--abbrev-ref", "HEAD"].as_slice(),
    ] {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&wt_path)
            .output()
            .await?;
        let candidate = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !candidate.is_empty() && candidate != "HEAD" {
            branch = candidate;
            break;
        }
    }
    if branch.is_empty() || branch == "HEAD" {
        anyhow::bail!("could not detect branch in {}", wt_path.display());
    }

    // Create task entry + write brief via gateway (daemon writes the brief).
    let client = DaemonClient::discover()?;
    let note_text =
        note.unwrap_or("Continue from current state. Run tests, fix failures, create PR.");
    let body = json!({
        "path": wt_path.to_string_lossy(),
        "title": title,
        "branch": branch,
        "note": note_text,
        "project": project,
    });
    let result = client.post("/api/captain/adopt", &body).await?;
    let id = result["id"].as_str().unwrap_or("?");

    println!("Adopted #{id}: {title}");
    println!("  Worktree: {}", wt_path.display());
    println!("  Branch:   {branch}");
    println!("Captain will pick this up on next tick.");
    Ok(())
}

async fn handle_nudge(id: &str, message: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"item_id": id, "message": message});
    let result = client.post("/api/captain/nudge", &body).await?;
    let worker = result["worker"].as_str().unwrap_or("?");
    let pid = result["pid"].as_u64().unwrap_or(0);
    println!("Nudged worker {worker} (pid {pid}) for item #{id}");
    Ok(())
}

async fn handle_captain_stop() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.post("/api/captain/stop", &json!({})).await?;
    let killed = result["killed"].as_u64().unwrap_or(0);
    println!("Killed {killed} worker process(es).");
    Ok(())
}

async fn handle_knowledge() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/knowledge").await?;
    let msg = result
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| serde_json::to_string_pretty(&result).unwrap_or_default());
    println!("{msg}");
    Ok(())
}

async fn handle_journal(worker: Option<&str>, limit: usize) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut url = format!("/api/journal?limit={limit}");
    if let Some(w) = worker {
        url.push_str(&format!("&worker={w}"));
    }
    let result = client.get(&url).await?;

    // Print totals.
    if let Some(totals) = result.get("totals") {
        let total = totals["total"].as_i64().unwrap_or(0);
        let successes = totals["successes"].as_i64().unwrap_or(0);
        let failures = totals["failures"].as_i64().unwrap_or(0);
        let unresolved = totals["unresolved"].as_i64().unwrap_or(0);
        println!("Journal: {total} total | {successes} success | {failures} failure | {unresolved} unresolved");
        println!("{}", "-".repeat(80));
    }

    // Print decisions.
    if let Some(decisions) = result["decisions"].as_array() {
        for d in decisions {
            let worker = d["worker"].as_str().unwrap_or("?");
            let action = d["action"].as_str().unwrap_or("?");
            let outcome = d["outcome"].as_str().unwrap_or("pending");
            let rule = d["rule"].as_str().unwrap_or("?");
            let rule_short: String = rule.chars().take(50).collect();
            let created = d["created_at"].as_str().unwrap_or("?");
            println!("{created}  {worker:<20} {action:<16} {outcome:<10} {rule_short}");
        }
    }
    Ok(())
}

async fn handle_patterns(list: bool, status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;

    if list {
        let mut url = "/api/patterns".to_string();
        if let Some(s) = status {
            url.push_str(&format!("?status={s}"));
        }
        let result = client.get(&url).await?;
        if let Some(patterns) = result["patterns"].as_array() {
            if patterns.is_empty() {
                println!("No patterns found.");
            }
            for p in patterns {
                let id = p["id"].as_i64().unwrap_or(0);
                let status = p["status"].as_str().unwrap_or("?");
                let confidence = p["confidence"].as_f64().unwrap_or(0.0);
                let pattern = p["pattern"].as_str().unwrap_or("?");
                let recommendation = p["recommendation"].as_str().unwrap_or("?");
                println!("#{id} [{status}] (confidence: {:.0}%)", confidence * 100.0);
                println!("  Pattern: {pattern}");
                println!("  Recommendation: {recommendation}");
                println!();
            }
        }
    } else {
        println!("Running pattern distiller...");
        let result = client.post("/api/knowledge/learn", &json!({})).await?;
        let summary = result["summary"].as_str().unwrap_or("Done.");
        let found = result["patterns_found"].as_i64().unwrap_or(0);
        println!("{summary}");
        if found > 0 {
            if let Some(patterns) = result["patterns"].as_array() {
                println!();
                for p in patterns {
                    let pattern = p["pattern"].as_str().unwrap_or("?");
                    let recommendation = p["recommendation"].as_str().unwrap_or("?");
                    let confidence = p["confidence"].as_f64().unwrap_or(0.0);
                    println!(
                        "  - {pattern} (confidence: {:.0}%)\n    → {recommendation}",
                        confidence * 100.0
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn handle_merge_pr(pr_num: &str, project: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({"pr_num": pr_num});
    if let Some(p) = project {
        body["project"] = json!(p);
    }
    let result = client.post("/api/captain/merge", &body).await?;
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
        Captain(CaptainArgs),
    }

    #[test]
    fn parse_tick_dry_run() {
        let cli = TestCli::try_parse_from(["test", "captain", "tick", "--dry-run"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Tick { dry_run, .. } => assert!(dry_run),
                _ => panic!("expected Tick"),
            },
        }
    }

    #[test]
    fn parse_workers_watch() {
        let cli = TestCli::try_parse_from(["test", "captain", "workers", "-w", "-n", "2"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Workers { watch, interval } => {
                    assert!(watch);
                    assert_eq!(interval, Some(2));
                }
                _ => panic!("expected Workers"),
            },
        }
    }

    #[test]
    fn parse_merge() {
        let cli =
            TestCli::try_parse_from(["test", "captain", "merge", "123", "-p", "mando"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Merge { pr_num, project } => {
                    assert_eq!(pr_num, "123");
                    assert_eq!(project.as_deref(), Some("mando"));
                }
                _ => panic!("expected Merge"),
            },
        }
    }

    #[test]
    fn parse_triage_no_args() {
        let cli = TestCli::try_parse_from(["test", "captain", "triage"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Triage { item_id } => {
                    assert!(item_id.is_none());
                }
                _ => panic!("expected Triage"),
            },
        }
    }

    #[test]
    fn parse_retry() {
        let cli = TestCli::try_parse_from(["test", "captain", "retry", "42"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Retry { id } => {
                    assert_eq!(id, "42");
                }
                _ => panic!("expected Retry"),
            },
        }
    }

    #[test]
    fn parse_triage_with_item_id() {
        let cli = TestCli::try_parse_from(["test", "captain", "triage", "ENG-123"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Triage { item_id } => {
                    assert_eq!(item_id.as_deref(), Some("ENG-123"));
                }
                _ => panic!("expected Triage"),
            },
        }
    }

    #[test]
    fn parse_adopt_with_project() {
        let cli = TestCli::try_parse_from([
            "test",
            "captain",
            "adopt",
            "Finish branch",
            "-w",
            "/tmp/worktree",
            "-n",
            "Carry on",
            "-p",
            "sandbox",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Adopt {
                    title,
                    worktree,
                    note,
                    project,
                } => {
                    assert_eq!(title, "Finish branch");
                    assert_eq!(worktree.as_deref(), Some("/tmp/worktree"));
                    assert_eq!(note.as_deref(), Some("Carry on"));
                    assert_eq!(project.as_deref(), Some("sandbox"));
                }
                _ => panic!("expected Adopt"),
            },
        }
    }

    #[test]
    fn parse_handoff() {
        let cli = TestCli::try_parse_from(["test", "captain", "handoff", "42"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Handoff { id } => {
                    assert_eq!(id, "42");
                }
                _ => panic!("expected Handoff"),
            },
        }
    }
}
