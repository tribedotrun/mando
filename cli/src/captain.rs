//! `mando captain` — tick loop and worker management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

use crate::captain_review::{
    handle_journal, handle_knowledge, handle_patterns, KnowledgeCommand, PatternsCommand,
};
use crate::http::{parse_id, DaemonClient};

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
    /// AI-scored triage of awaiting-review tasks by merge-readiness
    Triage {
        /// Optional specific task ID to triage
        item_id: Option<String>,
    },
    /// Reopen a completed/failed task with feedback
    Reopen {
        /// Task ID
        id: String,
        /// Feedback for the worker
        feedback: String,
    },
    /// Rework a task (fresh worktree, new worker)
    Rework {
        /// Task ID
        id: String,
        /// Feedback/instructions
        feedback: String,
    },
    /// Retry an errored task (re-trigger captain review)
    Retry {
        /// Task ID
        id: String,
    },
    /// Accept a no-PR task that is ready for human review
    Accept {
        /// Task ID
        id: String,
    },
    /// Hand off a task to human (worker -> human)
    Handoff {
        /// Task ID
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
        /// Task ID
        id: String,
        /// Nudge message to deliver to the worker
        message: String,
    },
    /// Graceful stop (kill all workers, drain tasks)
    Stop,
    /// Review and approve knowledge lessons
    Knowledge {
        #[command(subcommand)]
        command: Option<KnowledgeCommand>,
    },
    /// Query the decision journal
    Journal {
        /// Filter by worker name
        #[arg(short = 'w', long)]
        worker: Option<String>,
        /// Number of entries to show (default 20)
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
    /// Run the pattern distiller, list patterns, or review pattern status
    Patterns {
        #[command(subcommand)]
        command: Option<PatternsCommand>,
    },
}

pub(crate) async fn handle(args: CaptainArgs) -> anyhow::Result<()> {
    match args.command {
        CaptainCommand::Tick { dry_run } => handle_tick(dry_run).await,
        CaptainCommand::Workers { watch, interval } => handle_workers(watch, interval).await,
        CaptainCommand::Merge { pr_num, project } => {
            handle_merge_pr(&pr_num, project.as_deref()).await
        }
        CaptainCommand::Triage { item_id } => handle_triage_cmd(item_id.as_deref()).await,
        CaptainCommand::Reopen { id, feedback } => handle_reopen(&id, &feedback).await,
        CaptainCommand::Rework { id, feedback } => handle_rework(&id, &feedback).await,
        CaptainCommand::Retry { id } => handle_retry(&id).await,
        CaptainCommand::Accept { id } => handle_accept(&id).await,
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
        CaptainCommand::Knowledge { command } => handle_knowledge(command).await,
        CaptainCommand::Journal { worker, limit } => {
            handle_journal(worker.as_deref(), limit.unwrap_or(20)).await
        }
        CaptainCommand::Patterns { command } => handle_patterns(command).await,
    }
}

async fn handle_tick(dry_run: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"dry_run": dry_run});
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

pub(crate) async fn handle_triage_cmd(item_id: Option<&str>) -> anyhow::Result<()> {
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
    let client = DaemonClient::discover()?;
    let body = json!({"id": parse_id(id, "task")?, "feedback": feedback});
    client.post("/api/tasks/reopen", &body).await?;
    println!("Reopened task {id}");
    Ok(())
}

async fn handle_rework(id: &str, feedback: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": parse_id(id, "task")?, "feedback": feedback});
    client.post("/api/tasks/rework", &body).await?;
    println!("Rework requested for task {id}");
    Ok(())
}

async fn handle_retry(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": parse_id(id, "task")?});
    client.post("/api/tasks/retry", &body).await?;
    println!("Retried task {id} — re-entering captain review");
    Ok(())
}

async fn handle_accept(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": parse_id(id, "task")?});
    client.post("/api/tasks/accept", &body).await?;
    println!("Accepted task {id}");
    Ok(())
}

async fn handle_handoff(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": parse_id(id, "task")?});
    client.post("/api/tasks/handoff", &body).await?;
    println!("Handed off task {id} to human");
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

    if !wt_path.join(".git").exists() {
        anyhow::bail!("not a git worktree: {}", wt_path.display());
    }

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
    println!("Nudged worker {worker} (pid {pid}) for task #{id}");
    Ok(())
}

async fn handle_captain_stop() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.post("/api/captain/stop", &json!({})).await?;
    let killed = result["killed"].as_u64().unwrap_or(0);
    println!("Killed {killed} worker process(es).");
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

    #[test]
    fn parse_accept() {
        let cli = TestCli::try_parse_from(["test", "captain", "accept", "42"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Accept { id } => assert_eq!(id, "42"),
                _ => panic!("expected Accept"),
            },
        }
    }

    #[test]
    fn parse_knowledge_approve_all() {
        let cli =
            TestCli::try_parse_from(["test", "captain", "knowledge", "approve", "--all"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Knowledge { command } => match command {
                    Some(KnowledgeCommand::Approve { all, ids }) => {
                        assert!(all);
                        assert!(ids.is_empty());
                    }
                    _ => panic!("expected Knowledge approve"),
                },
                _ => panic!("expected Knowledge"),
            },
        }
    }

    #[test]
    fn parse_patterns_dismiss() {
        let cli = TestCli::try_parse_from(["test", "captain", "patterns", "dismiss", "7"]).unwrap();
        match cli.cmd {
            TestCmd::Captain(args) => match args.command {
                CaptainCommand::Patterns { command } => match command {
                    Some(PatternsCommand::Dismiss { id }) => assert_eq!(id, 7),
                    _ => panic!("expected Patterns dismiss"),
                },
                _ => panic!("expected Patterns"),
            },
        }
    }
}
