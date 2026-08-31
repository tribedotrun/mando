//! CLI subcommand: project management (add / edit / list / remove) — pure HTTP client.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Add a project to the config
    Add {
        /// Project name (unique short identifier)
        name: String,
        /// Local path to the project
        #[arg(short, long)]
        path: String,
        /// Short aliases for the project (e.g. -a sandbox -a sb)
        #[arg(short, long)]
        alias: Vec<String>,
    },
    /// Edit project config (path is immutable — remove and re-add instead)
    Edit {
        /// Project name or alias
        name: String,
        /// New display name
        #[arg(long)]
        rename: Option<String>,
        /// Override auto-detected GitHub repo (e.g. owner/repo)
        #[arg(long)]
        github_repo: Option<String>,
        /// Clear the GitHub repo association
        #[arg(long)]
        clear_github_repo: bool,
        /// Set aliases (replaces existing; comma-separated)
        #[arg(short, long)]
        alias: Option<Vec<String>>,
        /// Set worker preamble text
        #[arg(long)]
        preamble: Option<String>,
        /// Set check command (run after worker completes)
        #[arg(long)]
        check_command: Option<String>,
    },
    /// List configured projects
    List,
    /// Remove a project from the config
    Remove {
        /// Project name or alias
        name: String,
    },
}

pub(crate) async fn handle(args: ProjectArgs) -> Result<()> {
    match args.command {
        ProjectCommand::Add { name, path, alias } => handle_add(&name, &path, &alias).await,
        ProjectCommand::Edit {
            name,
            rename,
            github_repo,
            clear_github_repo,
            alias,
            preamble,
            check_command,
        } => {
            handle_edit(
                &name,
                rename.as_deref(),
                github_repo.as_deref(),
                clear_github_repo,
                alias.as_deref(),
                preamble.as_deref(),
                check_command.as_deref(),
            )
            .await
        }
        ProjectCommand::List => handle_list().await,
        ProjectCommand::Remove { name } => handle_remove(&name).await,
    }
}

async fn handle_add(name: &str, path: &str, aliases: &[String]) -> Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .post_projects(&api_types::AddProjectRequest {
            name: Some(name.to_string()),
            path: path.to_string(),
            aliases: aliases.to_vec(),
        })
        .await?;

    let abs_path = result.path.as_str();
    let github = result
        .github_repo
        .as_deref()
        .map(|r| format!(" (GitHub: {r})"))
        .unwrap_or_default();
    println!("Added project: {name} → {abs_path}{github}");
    Ok(())
}

async fn handle_edit(
    name: &str,
    rename: Option<&str>,
    github_repo: Option<&str>,
    clear_github_repo: bool,
    aliases: Option<&[String]>,
    preamble: Option<&str>,
    check_command: Option<&str>,
) -> Result<()> {
    let client = DaemonClient::discover()?;
    client
        .patch_projects_by_name(
            &api_types::ProjectNameParams {
                name: name.to_string(),
            },
            &api_types::EditProjectRequest {
                rename: rename.map(str::to_string),
                github_repo: github_repo.map(str::to_string),
                clear_github_repo: clear_github_repo.then_some(true),
                aliases: aliases.map(|v| v.to_vec()),
                hooks: None,
                preamble: preamble.map(str::to_string),
                check_command: check_command.map(str::to_string),
                scout_summary: None,
                redetect_logo: None,
            },
        )
        .await?;
    println!("Updated project: {name}");
    Ok(())
}

async fn handle_list() -> Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get_projects().await?;

    if result.projects.is_empty() {
        println!("No projects configured.");
    } else {
        for project in result.projects {
            let github = project
                .github_repo
                .as_deref()
                .map(|repo| format!(" (GitHub: {repo})"))
                .unwrap_or_default();
            let alias_str = if project.aliases.is_empty() {
                String::new()
            } else {
                format!(" [{}]", project.aliases.join(", "))
            };
            println!(
                "  {}{}{} → {}",
                project.name, alias_str, github, project.path
            );
        }
    }
    Ok(())
}

async fn handle_remove(name: &str) -> Result<()> {
    let client = DaemonClient::discover()?;
    client
        .delete_projects_by_name(&api_types::ProjectNameParams {
            name: name.to_string(),
        })
        .await?;
    println!("Removed project: {name}");
    Ok(())
}
