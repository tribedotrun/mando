//! Mando-specific files copied into freshly-created git worktrees.

use std::path::Path;

/// Copy gitignored local-only files that workers need.
pub(crate) async fn copy_local_files(repo_path: &Path, wt_path: &Path) {
    let local_files: &[&str] = &["claude.local.md", "devtools/mando-dev/dev.env.local"];
    for rel in local_files {
        let src = repo_path.join(rel);
        if src.exists() {
            let dst = wt_path.join(rel);
            if let Some(parent) = dst.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::warn!(
                        module = "worktree-bootstrap",
                        file = %rel,
                        error = %e,
                        "failed to create local file parent in worktree"
                    );
                    continue;
                }
            }
            if let Err(e) = tokio::fs::copy(&src, &dst).await {
                tracing::warn!(
                    module = "worktree-bootstrap",
                    file = %rel,
                    error = %e,
                    "failed to copy local file into worktree"
                );
            }
        }
    }
}
