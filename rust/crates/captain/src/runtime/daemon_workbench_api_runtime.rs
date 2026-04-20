use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use time::OffsetDateTime;

use super::CaptainRuntime;

fn layout_dir() -> PathBuf {
    global_types::data_dir().join("workbenches")
}

fn layout_path(workbench_id: i64) -> PathBuf {
    layout_dir().join(format!("{workbench_id}.json"))
}

fn read_layout(workbench_id: i64) -> anyhow::Result<crate::WorkbenchLayout> {
    let path = layout_path(workbench_id);
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(serde_json::from_str(&contents)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(crate::WorkbenchLayout::new()),
        Err(err) => Err(err.into()),
    }
}

fn write_layout(workbench_id: i64, layout: &crate::WorkbenchLayout) -> anyhow::Result<()> {
    let path = layout_path(workbench_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(layout)?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

fn merge_layout_patch(layout: &mut crate::WorkbenchLayout, patch: &Value) {
    if let Some(active_panel) = patch.get("activePanel").and_then(|value| value.as_str()) {
        layout.active_panel = Some(active_panel.to_string());
    }
    if let Some(order) = patch.get("panelOrder").and_then(|value| value.as_array()) {
        layout.panel_order = order
            .iter()
            .filter_map(|value| value.as_str().map(String::from))
            .collect();
    }
    if let Some(panels) = patch.get("panels").and_then(|value| value.as_object()) {
        for (key, value) in panels {
            if let Ok(panel) = serde_json::from_value::<crate::PanelState>(value.clone()) {
                layout.panels.insert(key.clone(), panel);
            }
        }
    }
}

fn repo_dir_name(project_path: &Path) -> String {
    project_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string()
}

enum PruneMetadataOutcome {
    Success,
    Failed(String),
}

async fn prune_worktree_metadata(project_path: &Path) -> PruneMetadataOutcome {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(project_path)
        .output()
        .await
    {
        Ok(output) => output,
        Err(err) => return PruneMetadataOutcome::Failed(err.to_string()),
    };

    if output.status.success() {
        PruneMetadataOutcome::Success
    } else {
        PruneMetadataOutcome::Failed(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

impl CaptainRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn current_worktree_branch(&self, wt_path: &Path) -> anyhow::Result<String> {
        crate::io::git::current_branch(wt_path).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_workbenches(&self, status: &str) -> anyhow::Result<Vec<crate::Workbench>> {
        match status {
            "archived" => crate::io::queries::workbenches::load_archived_only(&self.pool).await,
            "all" => crate::io::queries::workbenches::load_all(&self.pool).await,
            _ => crate::io::queries::workbenches::load_active(&self.pool).await,
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn patch_workbench(
        &self,
        id: i64,
        patch: crate::WorkbenchPatch,
    ) -> anyhow::Result<crate::WorkbenchPatchOutcome> {
        let Some(_) = crate::io::queries::workbenches::find_by_id(&self.pool, id).await? else {
            return Ok(crate::WorkbenchPatchOutcome::NotFound);
        };

        if let Some(title) = patch.title.as_deref() {
            crate::io::queries::workbenches::update_title(&self.pool, id, title).await?;
        }

        if let Some(archived) = patch.archived {
            if archived {
                crate::io::queries::workbenches::archive(&self.pool, id).await?;
            } else {
                crate::io::queries::workbenches::unarchive(&self.pool, id).await?;
            }
        }

        if let Some(pinned) = patch.pinned {
            let affected = if pinned {
                crate::io::queries::workbenches::pin(&self.pool, id).await?
            } else {
                crate::io::queries::workbenches::unpin(&self.pool, id).await?
            };
            if !affected {
                let message = if pinned {
                    "workbench cannot be pinned (archived or deleted)"
                } else {
                    "workbench not found"
                };
                return Ok(crate::WorkbenchPatchOutcome::Conflict(message.to_string()));
            }
        }

        let Some(updated) = crate::io::queries::workbenches::find_by_id(&self.pool, id).await?
        else {
            return Ok(crate::WorkbenchPatchOutcome::NotFound);
        };

        let item: Option<api_types::WorkbenchItem> =
            serde_json::from_value(serde_json::to_value(&updated).unwrap_or_default()).ok();
        self.bus.send(global_bus::BusPayload::Workbenches(Some(
            api_types::WorkbenchEventData {
                action: Some("updated".into()),
                item,
            },
        )));

        Ok(crate::WorkbenchPatchOutcome::Updated(updated))
    }

    #[tracing::instrument(skip_all)]
    pub async fn load_workbench_layout(
        &self,
        id: i64,
    ) -> anyhow::Result<Option<crate::WorkbenchLayout>> {
        let Some(_) = crate::io::queries::workbenches::find_by_id(&self.pool, id).await? else {
            return Ok(None);
        };

        let layout = tokio::task::spawn_blocking(move || read_layout(id)).await??;
        Ok(Some(layout))
    }

    #[tracing::instrument(skip_all)]
    pub async fn patch_workbench_layout(
        &self,
        id: i64,
        patch: Value,
    ) -> anyhow::Result<Option<crate::WorkbenchLayout>> {
        let Some(_) = crate::io::queries::workbenches::find_by_id(&self.pool, id).await? else {
            return Ok(None);
        };

        let layout = tokio::task::spawn_blocking(move || {
            let mut layout = read_layout(id)?;
            merge_layout_patch(&mut layout, &patch);
            write_layout(id, &layout)?;
            Ok::<_, anyhow::Error>(layout)
        })
        .await??;

        Ok(Some(layout))
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_worktree(
        &self,
        project: &str,
        name: Option<&str>,
    ) -> anyhow::Result<crate::CreateWorktreeOutcome> {
        let Some(project_row) = settings::projects::resolve(&self.pool, project).await? else {
            return Ok(crate::CreateWorktreeOutcome::ProjectNotFound(
                project.to_string(),
            ));
        };

        let project_path = global_infra::paths::expand_tilde(&project_row.path);
        let suffix = match name {
            Some(name) => name.to_string(),
            None => {
                let now = OffsetDateTime::now_utc();
                format!(
                    "{:02}{:02}-{:02}{:02}{:02}",
                    now.month() as u8,
                    now.day(),
                    now.hour(),
                    now.minute(),
                    now.second()
                )
            }
        };

        let branch = format!("worktree-{suffix}");
        let worktree_path = crate::io::git::worktree_path(&project_path, &suffix);

        crate::io::git::fetch_origin(&project_path).await?;
        let default_branch = crate::io::git::default_branch(&project_path).await?;

        if worktree_path.exists() {
            if let Err(err) = crate::io::git::remove_worktree(&project_path, &worktree_path).await {
                tracing::warn!(
                    module = "worktrees",
                    path = %worktree_path.display(),
                    error = %err,
                    "failed to remove stale worktree"
                );
                if worktree_path.exists() {
                    return Ok(crate::CreateWorktreeOutcome::Conflict(format!(
                        "worktree exists at {} and could not be removed: {err}",
                        worktree_path.display()
                    )));
                }
            }
        }
        if let Err(err) = crate::io::git::delete_local_branch(&project_path, &branch).await {
            tracing::debug!(
                module = "worktrees",
                branch = %branch,
                error = %err,
                "stale branch cleanup (expected if branch doesn't exist)"
            );
        }

        crate::io::git::create_worktree(&project_path, &branch, &worktree_path, &default_branch)
            .await?;

        let workbench = crate::Workbench::new(
            project_row.id,
            project_row.name.clone(),
            worktree_path.to_string_lossy().to_string(),
            suffix,
        );
        let workbench_id = match crate::io::queries::workbenches::insert(&self.pool, &workbench)
            .await
        {
            Ok(id) => Some(id),
            Err(err) => {
                tracing::warn!(
                    module = "worktrees",
                    path = %worktree_path.display(),
                    error = %err,
                    "workbench insert failed after worktree creation; cleaning up orphan worktree"
                );
                if let Err(remove_err) =
                    crate::io::git::remove_worktree(&project_path, &worktree_path).await
                {
                    tracing::warn!(
                        module = "worktrees",
                        path = %worktree_path.display(),
                        error = %remove_err,
                        "failed to clean up orphan worktree after workbench insert failure"
                    );
                }
                return Err(err);
            }
        };

        Ok(crate::CreateWorktreeOutcome::Created(
            crate::CreatedWorktree {
                path: worktree_path.to_string_lossy().to_string(),
                branch,
                project: project_row.name,
                workbench_id,
            },
        ))
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_worktrees(&self) -> anyhow::Result<Vec<crate::WorktreeEntry>> {
        let projects = settings::projects::list(&self.pool).await?;
        let mut worktrees = Vec::new();

        for row in &projects {
            if row.path.is_empty() {
                continue;
            }
            let project_path = global_infra::paths::expand_tilde(&row.path);
            match crate::io::git::list_worktrees(&project_path).await {
                Ok(paths) => {
                    worktrees.extend(paths.into_iter().map(|path| crate::WorktreeEntry {
                        project: row.name.clone(),
                        path: path.to_string_lossy().to_string(),
                    }));
                }
                Err(err) => {
                    tracing::warn!(
                        module = "worktrees",
                        project = row.name.as_str(),
                        error = %err,
                        "failed to list worktrees"
                    );
                }
            }
        }

        Ok(worktrees)
    }

    #[tracing::instrument(skip_all)]
    pub async fn prune_worktrees(&self) -> anyhow::Result<Vec<String>> {
        let projects = settings::projects::list(&self.pool).await?;
        let mut pruned = Vec::new();

        for row in &projects {
            if row.path.is_empty() {
                continue;
            }
            let project_path = global_infra::paths::expand_tilde(&row.path);
            match prune_worktree_metadata(&project_path).await {
                PruneMetadataOutcome::Success => pruned.push(row.name.clone()),
                PruneMetadataOutcome::Failed(stderr) => {
                    tracing::warn!(
                        module = "worktrees",
                        project = row.name.as_str(),
                        stderr = %stderr,
                        "git worktree prune failed"
                    );
                }
            }
        }

        Ok(pruned)
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_worktree(
        &self,
        worktree_path: &Path,
    ) -> anyhow::Result<crate::RemoveWorktreeOutcome> {
        let wt_dir = self.worktrees_dir();
        if !worktree_path.starts_with(&wt_dir) {
            return Ok(crate::RemoveWorktreeOutcome::NotFound);
        }

        let Some(worktree_name) = worktree_path.file_name().and_then(|name| name.to_str()) else {
            return Ok(crate::RemoveWorktreeOutcome::NotFound);
        };

        let projects = match settings::projects::list(&self.pool).await {
            Ok(projects) => projects,
            Err(err) => {
                tracing::error!(module = "captain-runtime-daemon_workbench_api_runtime", error = %err, "failed to list projects for worktree lookup");
                return Ok(crate::RemoveWorktreeOutcome::NotFound);
            }
        };
        let mut owner: Option<(usize, PathBuf)> = None;
        for row in &projects {
            if row.path.is_empty() {
                continue;
            }
            let project_path = global_infra::paths::expand_tilde(&row.path);
            let prefix = format!("{}-", repo_dir_name(&project_path));
            if worktree_name.starts_with(&prefix)
                && owner
                    .as_ref()
                    .is_none_or(|(best_len, _)| prefix.len() > *best_len)
            {
                owner = Some((prefix.len(), project_path));
            }
        }

        let Some((_, repo_path)) = owner else {
            return Ok(crate::RemoveWorktreeOutcome::NotFound);
        };

        crate::io::git::remove_worktree(&repo_path, worktree_path).await?;
        Ok(crate::RemoveWorktreeOutcome::Removed)
    }

    #[tracing::instrument(skip_all)]
    pub async fn cleanup_worktrees(
        &self,
        dry_run: bool,
    ) -> anyhow::Result<crate::CleanupWorktreesReport> {
        let projects = settings::projects::list(&self.pool).await?;
        let mut report = crate::CleanupWorktreesReport::default();
        let mut all_tracked = HashSet::new();
        let mut project_prefixes = Vec::new();

        for row in &projects {
            if row.path.is_empty() {
                continue;
            }
            let project_path = global_infra::paths::expand_tilde(&row.path);
            project_prefixes.push(format!("{}-", repo_dir_name(&project_path)));

            if !dry_run {
                match prune_worktree_metadata(&project_path).await {
                    PruneMetadataOutcome::Success => {}
                    PruneMetadataOutcome::Failed(stderr) => {
                        tracing::warn!(
                            module = "worktrees",
                            project = %row.name,
                            stderr = %stderr,
                            "git worktree prune (cleanup) failed"
                        );
                        report.prune_errors.push(crate::WorktreePruneError {
                            project: row.name.clone(),
                            error: stderr,
                        });
                    }
                }
            }

            match crate::io::git::list_worktrees(&project_path).await {
                Ok(paths) => {
                    all_tracked.extend(paths);
                }
                Err(err) => {
                    tracing::warn!(
                        module = "worktrees",
                        project = %row.name,
                        error = %err,
                        "failed to list worktrees, skipping project in orphan scan"
                    );
                }
            }
        }

        let wt_dir = self.worktrees_dir();
        let mut entries = match tokio::fs::read_dir(&wt_dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(report),
            Err(err) => {
                tracing::error!(
                    module = "worktrees",
                    dir = %wt_dir.display(),
                    error = %err,
                    "failed to read worktrees dir"
                );
                return Err(err.into());
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let owned = project_prefixes
                .iter()
                .filter(|prefix| dir_name.starts_with(prefix.as_str()))
                .max_by_key(|prefix| prefix.len())
                .is_some();
            if !owned || all_tracked.contains(&path) {
                continue;
            }

            let path_str = path.to_string_lossy().to_string();
            report.orphans.push(path_str.clone());

            if !dry_run {
                if let Err(err) = tokio::fs::remove_dir_all(&path).await {
                    tracing::warn!(
                        module = "worktrees",
                        path = %path.display(),
                        error = %err,
                        "failed to remove orphan worktree dir"
                    );
                } else {
                    report.removed.push(path_str);
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_layout_patch;

    #[test]
    fn merge_layout_patch_updates_known_fields_only() {
        let mut layout = crate::WorkbenchLayout::new();
        merge_layout_patch(
            &mut layout,
            &json!({
                "activePanel": "p2",
                "panelOrder": ["p2", "p1"],
                "panels": {
                    "p2": { "agent": "codex", "createdAt": 42 },
                    "bad": { "agent": true }
                }
            }),
        );

        assert_eq!(layout.active_panel.as_deref(), Some("p2"));
        assert_eq!(layout.panel_order, vec!["p2".to_string(), "p1".to_string()]);
        assert_eq!(
            layout.panels.get("p2").map(|panel| panel.agent.as_str()),
            Some("codex")
        );
        assert!(!layout.panels.contains_key("bad"));
    }
}
