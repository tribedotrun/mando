//! `mando worktree` — git worktree management CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct WorktreeArgs {
    #[command(subcommand)]
    pub command: WorktreeCommand,
}

#[derive(Subcommand)]
pub(crate) enum WorktreeCommand {
    /// Create a worktree and optionally launch claude
    Open {
        /// Worktree name or branch suffix
        name: Option<String>,
        /// Project name
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
    },
    /// List all worktrees
    List,
    /// Prune stale/untracked worktrees
    Prune,
    /// Remove a specific worktree
    Remove {
        /// Worktree path
        path: String,
    },
    /// Clean up stale worktrees (prune + remove merged branches)
    Cleanup {
        /// Dry-run mode (show what would be cleaned, don't act)
        #[arg(long)]
        dry_run: bool,
    },
}

pub(crate) async fn handle(args: WorktreeArgs) -> anyhow::Result<()> {
    match args.command {
        WorktreeCommand::Open { name, project } => handle_open(name, project).await,
        WorktreeCommand::List => handle_list().await,
        WorktreeCommand::Prune => handle_prune().await,
        WorktreeCommand::Remove { path } => handle_remove(&path).await,
        WorktreeCommand::Cleanup { dry_run } => handle_cleanup(dry_run).await,
    }
}

async fn handle_open(name: Option<String>, project: Option<String>) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;

    let client = DaemonClient::discover()?;
    let result = client
        .post_worktrees(&api_types::CreateWorktreeRequest { name, project })
        .await?;
    let wt_path = result.path;
    let branch = result.branch;
    let project_name = result.project;

    eprintln!("Worktree: {wt_path} (branch {branch}) for {project_name}");

    // Launch claude in the worktree if available (replaces this process).
    let claude_available = std::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok();
    if claude_available {
        // Pull a credential from the daemon pool. Best-effort: on any
        // failure (transport error, no usable credential), fall through to
        // ambient ~/.claude/ login — mirrors the `mando credentials pick`
        // shell wrapper semantics so the spawn behaves the same whether
        // the user runs `mando worktree open` directly or via the wrapper.
        let credential = client
            .post_credentials_pick(&api_types::CredentialPickRequest {
                id: None,
                label: None,
            })
            .await
            .ok()
            .and_then(|r| r.pick);

        eprintln!("Launching claude...");
        let mut cmd = std::process::Command::new("claude");
        cmd.arg("--dangerously-skip-permissions")
            .arg("--effort")
            .arg("max")
            .current_dir(wt_path);
        if let Some(pick) = credential {
            eprintln!("mando: using credential '{}' (#{})", pick.label, pick.id);
            cmd.env("CLAUDE_CODE_OAUTH_TOKEN", pick.token);
            cmd.env("MANDO_CREDENTIAL_LABEL", &pick.label);
            cmd.env("MANDO_CREDENTIAL_ID", pick.id.to_string());
        }
        let err = cmd.exec();
        anyhow::bail!("failed to exec claude: {err}");
    }
    eprintln!("Worktree ready at {wt_path}");
    Ok(())
}

async fn handle_list() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get_worktrees().await?;

    match result.worktrees.as_slice() {
        [] => println!("No worktrees found."),
        wts => {
            let mut current_project = "";
            for wt in wts {
                if wt.project != current_project {
                    if !current_project.is_empty() {
                        println!();
                    }
                    println!("Project: {}", wt.project);
                    println!("{}", "-".repeat(50));
                    current_project = &wt.project;
                }
                println!("  {}", wt.path);
            }
        }
    }
    Ok(())
}

async fn handle_prune() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.post_worktrees_prune().await?;
    let pruned = result.pruned.len();
    println!("Pruned stale worktrees for {pruned} project(s).");
    Ok(())
}

async fn handle_remove(path: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_worktrees_remove(&api_types::RemoveWorktreeRequest {
            path: path.to_string(),
        })
        .await?;
    println!("Removed worktree at {path}.");
    Ok(())
}

async fn handle_cleanup(dry_run: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .post_worktrees_cleanup(&api_types::WorktreeCleanupRequest { dry_run })
        .await?;

    if result.orphans.is_empty() {
        println!("No orphan worktrees found.");
    } else if dry_run {
        println!("Orphan worktrees (dry run):");
        for orphan in result.orphans {
            println!("  {orphan}");
        }
    } else {
        for orphan in result.orphans {
            println!("Removed orphan: {orphan}");
        }
    }
    Ok(())
}
