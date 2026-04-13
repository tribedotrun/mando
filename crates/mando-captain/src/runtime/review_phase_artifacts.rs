//! Artifact gate computation for the review phase.

/// Artifact gate results computed from DB.
pub(crate) struct ArtifactGates {
    pub has_evidence: bool,
    pub evidence_fresh: bool,
    pub has_work_summary: bool,
    pub work_summary_fresh: bool,
}

/// Query task_artifacts to compute artifact gates for a single task.
pub(crate) async fn compute_artifact_gates(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    reopen_seq: i64,
    reopened_at: Option<&str>,
) -> ArtifactGates {
    let artifacts = mando_db::queries::artifacts::list_for_task(pool, task_id)
        .await
        .unwrap_or_default();

    let evidence_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| a.artifact_type == mando_types::ArtifactType::Evidence)
        .collect();
    let summary_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| a.artifact_type == mando_types::ArtifactType::WorkSummary)
        .collect();

    let has_evidence = !evidence_artifacts.is_empty();
    let has_work_summary = !summary_artifacts.is_empty();

    let evidence_fresh = if reopen_seq == 0 || reopened_at.is_none() {
        has_evidence
    } else {
        let threshold = reopened_at.unwrap_or("");
        evidence_artifacts
            .iter()
            .any(|a| a.created_at.as_str() > threshold)
    };

    let work_summary_fresh = if reopen_seq == 0 || reopened_at.is_none() {
        has_work_summary
    } else {
        let threshold = reopened_at.unwrap_or("");
        summary_artifacts
            .iter()
            .any(|a| a.created_at.as_str() > threshold)
    };

    ArtifactGates {
        has_evidence,
        evidence_fresh,
        has_work_summary,
        work_summary_fresh,
    }
}
