use super::CaptainRuntime;

impl CaptainRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn create_evidence_artifact(
        &self,
        task_id: i64,
        files: &[crate::EvidenceFileSpec],
    ) -> anyhow::Result<crate::EvidenceArtifactCreated> {
        let content = format!("Evidence ({} files)", files.len());
        let artifact_id = crate::io::queries::artifacts::insert(
            &self.pool,
            task_id,
            crate::ArtifactType::Evidence,
            &content,
            &[],
        )
        .await?;

        let media: Vec<crate::ArtifactMedia> = files
            .iter()
            .enumerate()
            .map(|(index, file)| crate::ArtifactMedia {
                index: index as u32,
                filename: file.filename.clone(),
                ext: file.ext.clone(),
                local_path: Some(format!(
                    "artifacts/{task_id}/{artifact_id}-{index}.{}",
                    file.ext
                )),
                remote_url: None,
                caption: Some(file.caption.clone()),
                kind: file.kind,
            })
            .collect();

        crate::io::queries::artifacts::update_media(&self.pool, artifact_id, &media).await?;
        self.bus.send(global_bus::BusPayload::Artifacts(Some(
            api_types::ArtifactEventData {
                action: "evidence_created".into(),
                task_id,
                artifact_id,
            },
        )));

        Ok(crate::EvidenceArtifactCreated { artifact_id, media })
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_work_summary_artifact(
        &self,
        task_id: i64,
        content: &str,
    ) -> anyhow::Result<i64> {
        let artifact_id = crate::io::queries::artifacts::insert(
            &self.pool,
            task_id,
            crate::ArtifactType::WorkSummary,
            content,
            &[],
        )
        .await?;

        self.bus.send(global_bus::BusPayload::Artifacts(Some(
            api_types::ArtifactEventData {
                action: "summary_created".into(),
                task_id,
                artifact_id,
            },
        )));

        Ok(artifact_id)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_artifact(
        &self,
        artifact_id: i64,
    ) -> anyhow::Result<Option<crate::TaskArtifact>> {
        crate::io::queries::artifacts::get(&self.pool, artifact_id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_artifact_media(
        &self,
        artifact_id: i64,
        patches: &[(u32, String)],
    ) -> anyhow::Result<crate::UpdateArtifactMediaOutcome> {
        let Some(artifact) = crate::io::queries::artifacts::get(&self.pool, artifact_id).await?
        else {
            return Ok(crate::UpdateArtifactMediaOutcome::ArtifactNotFound);
        };

        let mut merged = artifact.media.clone();
        for (index, remote_url) in patches {
            let Some(slot) = merged.iter_mut().find(|media| media.index == *index) else {
                return Ok(crate::UpdateArtifactMediaOutcome::MediaIndexNotFound(
                    *index,
                ));
            };
            slot.remote_url = Some(remote_url.clone());
        }

        crate::io::queries::artifacts::update_media(&self.pool, artifact_id, &merged).await?;
        Ok(crate::UpdateArtifactMediaOutcome::Updated)
    }

    #[tracing::instrument(skip_all)]
    pub async fn activity_stats(&self, days: u32) -> anyhow::Result<(i64, Vec<(String, i64)>)> {
        let store = self.task_store.read().await;
        let rows = store.daily_merge_counts(days).await?;

        let today_str = store.today_localtime_iso().await?;
        let today = time::Date::parse(
            &today_str,
            &time::format_description::well_known::Iso8601::DATE,
        )?;
        let cutoff_7d = today - time::Duration::days(7);

        let mut merged_7d = 0;
        for (date_str, count) in &rows {
            if let Ok(date) = time::Date::parse(
                date_str,
                &time::format_description::well_known::Iso8601::DATE,
            ) {
                if date >= cutoff_7d {
                    merged_7d += count;
                }
            }
        }

        Ok((merged_7d, rows))
    }

    #[tracing::instrument(skip_all)]
    pub async fn load_sse_snapshot_data(
        &self,
    ) -> anyhow::Result<(
        Vec<crate::Task>,
        Vec<api_types::WorkerDetail>,
        Vec<crate::Workbench>,
    )> {
        let workflow = self.settings.load_captain_workflow();
        let store = self.task_store.read().await;
        let all_items = store.load_all().await?;
        drop(store);
        let health_path = crate::config::worker_health_path();
        let health = crate::io::health_store::load_health_state_async(&health_path).await?;
        let nudge_budget = workflow.agent.max_interventions;

        let workers = all_items
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    crate::ItemStatus::InProgress
                        | crate::ItemStatus::CaptainReviewing
                        | crate::ItemStatus::CaptainMerging
                ) && task.worker.is_some()
            })
            .map(|task| {
                let worker_name = task.worker.as_deref().unwrap_or("");
                let nudge_count =
                    crate::io::health_store::get_health_u32(&health, worker_name, "nudge_count");
                let last_action = health
                    .get(worker_name)
                    .and_then(|value| value.get("last_action"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                api_types::WorkerDetail {
                    id: task.id,
                    title: task.title.clone(),
                    status: serde_json::from_value(
                        serde_json::to_value(task.status).unwrap_or_default(),
                    )
                    .ok(),
                    project: task.project.clone(),
                    github_repo: task.github_repo.clone(),
                    branch: task.branch.clone(),
                    cc_session_id: task.session_ids.worker.clone(),
                    worker: task.worker.clone(),
                    worktree: task.worktree.clone(),
                    pr_number: task.pr_number,
                    started_at: task.worker_started_at.clone(),
                    last_activity_at: task.last_activity_at.clone(),
                    intervention_count: Some(task.intervention_count),
                    nudge_count: Some(nudge_count),
                    nudge_budget: Some(nudge_budget),
                    last_action: Some(last_action.to_string()),
                    pid: None,
                    is_stale: None,
                }
            })
            .collect();

        let workbenches = match crate::io::queries::workbenches::load_active(&self.pool).await {
            Ok(workbenches) => workbenches,
            Err(err) => {
                tracing::warn!(module = "captain-runtime-daemon_transport_runtime", error = %err, "SSE snapshot: failed to load workbenches");
                Vec::new()
            }
        };

        Ok((all_items, workers, workbenches))
    }
}
