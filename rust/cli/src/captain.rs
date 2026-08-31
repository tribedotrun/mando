//! `mando captain` — tick loop and worker management CLI (HTTP client).

use clap::{Args, Subcommand};

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
    let result = client
        .post_captain_tick(&api_types::TickRequest {
            dry_run: Some(dry_run),
            emit_notifications: Some(true),
            until_idle: None,
            max_ticks: None,
            until_status: None,
            task_id: None,
        })
        .await?;
    println!("{}", serde_json::to_string_pretty(&result.last)?);
    Ok(())
}

async fn handle_workers(watch: bool, interval: Option<u64>) -> anyhow::Result<()> {
    let interval_secs = interval.unwrap_or(5);
    let client = DaemonClient::discover()?;
    loop {
        let health = client
            .accepting_server_error_bodies()
            .get_health_system()
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
    let result = client
        .post_captain_triage(&api_types::TriageRequest {
            item_id: item_id.map(str::to_string),
        })
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_reopen(id: &str, feedback: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_reopen(&api_types::TaskFeedbackRequest {
            id: parse_id(id, "task")?,
            feedback: feedback.to_string(),
        })
        .await?;
    println!("Reopened task {id}");
    Ok(())
}

async fn handle_rework(id: &str, feedback: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_rework(&api_types::TaskFeedbackRequest {
            id: parse_id(id, "task")?,
            feedback: feedback.to_string(),
        })
        .await?;
    println!("Rework requested for task {id}");
    Ok(())
}

async fn handle_retry(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_retry(&api_types::TaskIdRequest {
            id: parse_id(id, "task")?,
        })
        .await?;
    println!("Retried task {id} — re-entering captain review");
    Ok(())
}

async fn handle_accept(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_accept(&api_types::TaskIdRequest {
            id: parse_id(id, "task")?,
        })
        .await?;
    println!("Accepted task {id}");
    Ok(())
}

async fn handle_handoff(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_handoff(&api_types::TaskIdRequest {
            id: parse_id(id, "task")?,
        })
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
    let result = client
        .post_captain_adopt(&api_types::AdoptRequest {
            title: title.to_string(),
            worktree_path: wt_path.to_string_lossy().into_owned(),
            note: Some(note_text.to_string()),
            project: project.map(str::to_string),
        })
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
    let result = client
        .post_captain_nudge(&api_types::NudgeRequest {
            item_id: id.to_string(),
            message: message.to_string(),
        })
        .await?;
    let worker = result.worker.as_deref().unwrap_or("?");
    let pid = result.pid.unwrap_or(0);
    println!("Nudged worker {worker} (pid {pid}) for task #{id}");
    Ok(())
}

async fn handle_captain_stop() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.post_captain_stop().await?;
    let killed = result.killed;
    println!("Killed {killed} worker process(es).");
    Ok(())
}

async fn handle_stop_task(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_tasks_stop(&api_types::TaskIdRequest {
            id: parse_id(id, "task")?,
        })
        .await?;
    println!("Stopped task {id}. Worktree preserved; reopen to resume.");
    Ok(())
}

pub(crate) async fn handle_merge_pr(pr_num: &str, project: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let pr_number =
        parse_pr_number(pr_num).ok_or_else(|| anyhow::anyhow!("invalid PR reference: {pr_num}"))?;
    let result = client
        .post_tasks_merge(&api_types::MergeRequest {
            pr_number,
            project: project.unwrap_or("").to_string(),
        })
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
