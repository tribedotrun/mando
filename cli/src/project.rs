//! CLI subcommand: project management (add / edit / list / remove) — pure HTTP client.

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;

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
        } => {
            handle_edit(
                &name,
                rename.as_deref(),
                github_repo.as_deref(),
                clear_github_repo,
                alias.as_deref(),
                preamble.as_deref(),
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
        .post(
            "/api/projects",
            &json!({
                "name": name,
                "path": path,
                "aliases": aliases,
            }),
        )
        .await?;

    let abs_path = result["path"].as_str().unwrap_or(path);
    let github = result["githubRepo"]
        .as_str()
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
) -> Result<()> {
    let client = DaemonClient::discover()?;
    let mut body = json!({});
    if let Some(new_name) = rename {
        body["rename"] = json!(new_name);
    }
    if let Some(repo) = github_repo {
        body["github_repo"] = json!(repo);
    }
    if clear_github_repo {
        body["clear_github_repo"] = json!(true);
    }
    if let Some(alias_list) = aliases {
        body["aliases"] = json!(alias_list);
    }
    if let Some(pre) = preamble {
        body["preamble"] = json!(pre);
    }

    let encoded = urlencoding::encode(name);
    client
        .patch(&format!("/api/projects/{encoded}"), &body)
        .await?;
    println!("Updated project: {name}");
    Ok(())
}

async fn handle_list() -> Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/projects").await?;

    let projects = result["projects"].as_array();
    match projects {
        Some(ps) if !ps.is_empty() => {
            for p in ps {
                let name = p["name"].as_str().unwrap_or("?");
                let path = p["path"].as_str().unwrap_or("?");
                let github = p["githubRepo"]
                    .as_str()
                    .map(|r| format!(" (GitHub: {r})"))
                    .unwrap_or_default();
                let aliases: Vec<&str> = p["aliases"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let alias_str = if aliases.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", aliases.join(", "))
                };
                println!("  {name}{alias_str}{github} → {path}");
            }
        }
        _ => println!("No projects configured."),
    }
    Ok(())
}

async fn handle_remove(name: &str) -> Result<()> {
    let client = DaemonClient::discover()?;
    let encoded = urlencoding::encode(name);
    client.delete(&format!("/api/projects/{encoded}")).await?;
    println!("Removed project: {name}");
    Ok(())
}
