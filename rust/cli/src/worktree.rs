//! `mando worktree` — git worktree management CLI (HTTP client).

use clap::{Args, Subcommand};

use crate::gateway_paths as paths;
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
    let result: api_types::CreateWorktreeResponse = client
        .post_json(
            paths::WORKTREES,
            &api_types::CreateWorktreeRequest { name, project },
        )
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
        eprintln!("Launching claude...");
        let err = std::process::Command::new("claude")
            .arg("--dangerously-skip-permissions")
            .arg("--effort")
            .arg("max")
            .current_dir(wt_path)
            .exec();
        anyhow::bail!("failed to exec claude: {err}");
    }
    eprintln!("Worktree ready at {wt_path}");
    Ok(())
}

async fn handle_list() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::WorktreeListResponse = client.get_json(paths::WORKTREES).await?;

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
    let result: api_types::WorktreePruneResponse =
        client.post_no_body(paths::WORKTREES_PRUNE).await?;
    let pruned = result.pruned.len();
    println!("Pruned stale worktrees for {pruned} project(s).");
    Ok(())
}

async fn handle_remove(path: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::BoolOkResponse, _>(
            paths::WORKTREES_REMOVE,
            &api_types::RemoveWorktreeRequest {
                path: path.to_string(),
            },
        )
        .await?;
    println!("Removed worktree at {path}.");
    Ok(())
}

async fn handle_cleanup(dry_run: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result: api_types::WorktreeCleanupResponse = client
        .post_json(
            paths::WORKTREES_CLEANUP,
            &api_types::WorktreeCleanupRequest { dry_run },
        )
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
        Worktree(WorktreeArgs),
    }

    #[test]
    fn parse_worktree_list() {
        let cli = TestCli::try_parse_from(["test", "worktree", "list"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => {
                assert!(matches!(args.command, WorktreeCommand::List));
            }
        }
    }

    #[test]
    fn parse_worktree_open() {
        let cli = TestCli::try_parse_from(["test", "worktree", "open", "my-feature"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => match args.command {
                WorktreeCommand::Open { name, project } => {
                    assert_eq!(name.as_deref(), Some("my-feature"));
                    assert!(project.is_none());
                }
                _ => panic!("expected Open"),
            },
        }
    }

    #[test]
    fn parse_worktree_open_with_project() {
        let cli =
            TestCli::try_parse_from(["test", "worktree", "open", "fix", "-p", "mando"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => match args.command {
                WorktreeCommand::Open { name, project } => {
                    assert_eq!(name.as_deref(), Some("fix"));
                    assert_eq!(project.as_deref(), Some("mando"));
                }
                _ => panic!("expected Open"),
            },
        }
    }

    #[test]
    fn parse_worktree_prune() {
        let cli = TestCli::try_parse_from(["test", "worktree", "prune"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => {
                assert!(matches!(args.command, WorktreeCommand::Prune));
            }
        }
    }

    #[test]
    fn parse_worktree_remove() {
        let cli = TestCli::try_parse_from(["test", "worktree", "remove", "/tmp/wt"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => match args.command {
                WorktreeCommand::Remove { path } => {
                    assert_eq!(path, "/tmp/wt");
                }
                _ => panic!("expected Remove"),
            },
        }
    }

    #[test]
    fn parse_worktree_cleanup() {
        let cli = TestCli::try_parse_from(["test", "worktree", "cleanup"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => match args.command {
                WorktreeCommand::Cleanup { dry_run } => {
                    assert!(!dry_run);
                }
                _ => panic!("expected Cleanup"),
            },
        }
    }

    #[test]
    fn parse_worktree_cleanup_dry_run() {
        let cli = TestCli::try_parse_from(["test", "worktree", "cleanup", "--dry-run"]).unwrap();
        match cli.cmd {
            TestCmd::Worktree(args) => match args.command {
                WorktreeCommand::Cleanup { dry_run } => {
                    assert!(dry_run);
                }
                _ => panic!("expected Cleanup"),
            },
        }
    }
}
