//! `mando captain` — tick loop and worker management CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::gateway_paths as paths;
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
    /// Rework a task (same worktree, new branch, new worker)
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
    /// Stop one task (if ID provided) or drain all workers globally.
    Stop {
        /// Task ID — omit to stop all workers globally.
        id: Option<String>,
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
        CaptainCommand::Stop { id } => match id {
            Some(task_id) => handle_stop_task(&task_id).await,
            None => handle_captain_stop().await,
        },
    }
}

async fn handle_tick(dry_run: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::TickDrainResult = client
        .post_json(
            paths::CAPTAIN_TICK,
            &api_types::TickRequest {
                dry_run: Some(dry_run),
                emit_notifications: Some(true),
                until_idle: None,
                max_ticks: None,
                until_status: None,
                task_id: None,
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&result.last)?);
    Ok(())
}

async fn handle_workers(watch: bool, interval: Option<u64>) -> anyhow::Result<()> {
    let interval_secs = interval.unwrap_or(5);
    let client = DaemonClient::discover()?;
    loop {
        let health: api_types::SystemHealthResponse = client
            .get_json_with_body_on_5xx(paths::HEALTH_SYSTEM)
            .await?;

        if watch {
            print!("\x1b[2J\x1b[H");
        }

        println!("Captain Workers");
        println!("{}", "-".repeat(40));
        println!("  Active workers: {}", health.active_workers);
        println!("  Tasks:          {}", health.total_items);
        println!("  Projects:       {}", health.projects.join(", "));

        if !watch {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
    Ok(())
}

pub(crate) async fn handle_triage_cmd(item_id: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::TriageResponse = client
        .post_json(
            paths::CAPTAIN_TRIAGE,
            &api_types::TriageRequest {
                item_id: item_id.map(str::to_string),
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_reopen(id: &str, feedback: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_REOPEN,
            &api_types::TaskFeedbackRequest {
                id: parse_id(id, "task")?,
                feedback: feedback.to_string(),
            },
        )
        .await?;
    println!("Reopened task {id}");
    Ok(())
}

async fn handle_rework(id: &str, feedback: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_REWORK,
            &api_types::TaskFeedbackRequest {
                id: parse_id(id, "task")?,
                feedback: feedback.to_string(),
            },
        )
        .await?;
    println!("Rework requested for task {id}");
    Ok(())
}

async fn handle_retry(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_RETRY,
            &api_types::TaskIdRequest {
                id: parse_id(id, "task")?,
            },
        )
        .await?;
    println!("Retried task {id} — re-entering captain review");
    Ok(())
}

async fn handle_accept(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_ACCEPT,
            &api_types::TaskIdRequest {
                id: parse_id(id, "task")?,
            },
        )
        .await?;
    println!("Accepted task {id}");
    Ok(())
}

async fn handle_handoff(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_HANDOFF,
            &api_types::TaskIdRequest {
                id: parse_id(id, "task")?,
            },
        )
        .await?;
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

    let branch = global_git::checked_out_branch(&wt_path).await?;
    if branch.is_empty() || branch == "HEAD" {
        anyhow::bail!("could not detect branch in {}", wt_path.display());
    }

    let client = DaemonClient::discover()?;
    let note_text =
        note.unwrap_or("Continue from current state. Run tests, fix failures, create PR.");
    let result: api_types::TaskCreateResponse = client
        .post_json(
            paths::CAPTAIN_ADOPT,
            &api_types::AdoptRequest {
                title: title.to_string(),
                worktree_path: wt_path.to_string_lossy().into_owned(),
                note: Some(note_text.to_string()),
                project: project.map(str::to_string),
            },
        )
        .await?;
    let id = result.id;

    println!("Adopted #{id}: {title}");
    println!("  Worktree: {}", wt_path.display());
    println!("  Branch:   {branch}");
    println!("Captain will pick this up on next tick.");
    Ok(())
}

async fn handle_nudge(id: &str, message: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::NudgeResponse = client
        .post_json(
            paths::CAPTAIN_NUDGE,
            &api_types::NudgeRequest {
                item_id: id.to_string(),
                message: message.to_string(),
            },
        )
        .await?;
    let worker = result.worker.as_deref().unwrap_or("?");
    let pid = result.pid.unwrap_or(0);
    println!("Nudged worker {worker} (pid {pid}) for task #{id}");
    Ok(())
}

async fn handle_captain_stop() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::StopWorkersResponse = client.post_no_body(paths::CAPTAIN_STOP).await?;
    let killed = result.killed;
    println!("Killed {killed} worker process(es).");
    Ok(())
}

async fn handle_stop_task(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::TASKS_STOP,
            &api_types::TaskIdRequest {
                id: parse_id(id, "task")?,
            },
        )
        .await?;
    println!("Stopped task {id}. Worktree preserved; reopen to resume.");
    Ok(())
}

pub(crate) async fn handle_merge_pr(pr_num: &str, project: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let pr_number =
        parse_pr_number(pr_num).ok_or_else(|| anyhow::anyhow!("invalid PR reference: {pr_num}"))?;
    let result: api_types::MergeResponse = client
        .post_json(
            paths::TASKS_MERGE,
            &api_types::MergeRequest {
                pr_number,
                project: project.unwrap_or("").to_string(),
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn parse_pr_number(pr: &str) -> Option<i64> {
    if let Some(idx) = pr.rfind("/pull/") {
        let after = &pr[idx + 6..];
        let num_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        return after[..num_end].parse().ok();
    }
    pr.trim_start_matches('#').parse().ok()
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
}
