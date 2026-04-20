use std::path::Path;

pub async fn is_git_repository(path: &Path) -> anyhow::Result<bool> {
    if tokio::fs::try_exists(path.join(".git"))
        .await
        .unwrap_or(false)
    {
        return Ok(true);
    }

    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .await?;
    Ok(output.status.success())
}
