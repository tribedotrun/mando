//! Task creation with the same multipart image/context contract as Electron.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use clap::Args;
use tokio::io::AsyncReadExt;

use crate::http::DaemonClient;

const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Args)]
pub(crate) struct AddTaskArgs {
    /// Task title
    title: String,
    /// Project name
    #[arg(short = 'p', long)]
    project: Option<String>,
    /// Plan/brief path for planned handoff (queues the task)
    #[arg(long)]
    plan: Option<String>,
    /// Task context / instructions
    #[arg(long, conflicts_with = "context_file")]
    context: Option<String>,
    /// Read task context from a UTF-8 file
    #[arg(long)]
    context_file: Option<PathBuf>,
    /// Attach a PNG, JPEG, GIF, or WebP file (repeat for multiple images; max 10 MiB each)
    #[arg(long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
    /// Task owner: claude or codex (default: daemon setting)
    #[arg(long)]
    provider: Option<api_types::TaskCreateProvider>,
    /// Override GLM implementation routing; false keeps implementation on the task provider
    #[arg(long, action = clap::ArgAction::Set)]
    glm_worker: Option<bool>,
    /// Mark as no-PR / research-only
    #[arg(long)]
    no_pr: bool,
    /// Disable auto-merge even if global auto-merge is on
    #[arg(long)]
    no_auto_merge: bool,
    /// Print the complete created task as JSON
    #[arg(long)]
    json: bool,
}

async fn image_part(path: &Path) -> anyhow::Result<reqwest::multipart::Part> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("image filename must be valid UTF-8")?;
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => anyhow::bail!(
            "unsupported image format: {} (use PNG, JPEG, GIF, or WebP)",
            path.display()
        ),
    };
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("cannot open image {}", path.display()))?;
    let metadata = file.metadata().await?;
    ensure!(
        metadata.is_file(),
        "image is not a regular file: {}",
        path.display()
    );
    ensure!(
        metadata.len() > 0 && metadata.len() <= MAX_IMAGE_BYTES,
        "image must be nonempty and at most 10 MiB: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_IMAGE_BYTES,
        "image must be nonempty and at most 10 MiB: {}",
        path.display()
    );
    Ok(reqwest::multipart::Part::bytes(bytes)
        .file_name(filename.to_owned())
        .mime_str(mime)?)
}

pub(crate) async fn handle_add(args: AddTaskArgs) -> anyhow::Result<()> {
    ensure!(
        !args.title.trim().is_empty(),
        "task title must not be empty"
    );
    let context = match args.context_file {
        Some(path) => Some(
            tokio::fs::read_to_string(&path)
                .await
                .with_context(|| format!("cannot read context file {}", path.display()))?,
        ),
        None => args.context,
    };
    let mut form = reqwest::multipart::Form::new()
        .text("title", args.title.clone())
        .text("source", "cli");
    for (name, value) in [
        ("project", args.project),
        ("context", context),
        ("plan", args.plan),
        (
            "provider",
            args.provider.map(|provider| provider.to_string()),
        ),
        (
            "use_glm_worker",
            args.glm_worker.map(|value| value.to_string()),
        ),
    ] {
        if let Some(value) = value {
            form = form.text(name, value);
        }
    }
    if args.no_pr {
        form = form.text("no_pr", "true");
    }
    if args.no_auto_merge {
        form = form.text("no_auto_merge", "true");
    }
    // Read every file before making a request: a missing later attachment must
    // not leave a partially-created task or upload behind.
    for path in &args.images {
        form = form.part("images", image_part(path).await?);
    }
    let client = DaemonClient::discover()?;
    let result = client.post_tasks_add_multipart(form).await?;
    if args.json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("Added item #{}: {}", result.id, result.title);
    }
    Ok(())
}
