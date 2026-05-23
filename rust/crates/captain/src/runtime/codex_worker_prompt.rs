use std::path::Path;

use anyhow::Result;
use settings::{CaptainWorkflow, ProjectConfig};

use crate::Task;

#[tracing::instrument(skip(item, project_config, workflow), fields(task_id = item.id, provider = "codex"))]
pub(super) async fn render_worker_prompt(
    item: &Task,
    branch: &str,
    wt_path: &Path,
    project_config: &ProjectConfig,
    workflow: &CaptainWorkflow,
) -> Result<String> {
    let item_clone = item.clone();
    let branch = branch.to_string();
    let wt = wt_path.to_path_buf();
    let project = project_config.clone();
    let workflow = workflow.clone();
    tokio::task::spawn_blocking(move || {
        let slot = branch
            .rsplit('-')
            .next()
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        super::spawner_prompt::prepare_initial_worker_prompt(
            &item_clone,
            slot,
            &branch,
            &wt,
            &project,
            &workflow,
        )
    })
    .await?
}
