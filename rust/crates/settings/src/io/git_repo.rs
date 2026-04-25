use std::path::Path;

pub async fn is_git_repository(path: &Path) -> anyhow::Result<bool> {
    global_git::is_repository(path).await
}
