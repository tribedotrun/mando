//! Artifact gate computation for the review phase.

pub(crate) const SCREENSHOT_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];
pub(crate) const RECORDING_EXTS: &[&str] = &["gif", "mp4", "mov", "webm"];

/// Artifact gate results computed from DB.
pub(crate) struct ArtifactGates {
    pub has_evidence: bool,
    pub evidence_fresh: bool,
    pub has_work_summary: bool,
    pub work_summary_fresh: bool,
    /// At least one screenshot (png/jpg/jpeg/webp) in evidence artifacts.
    pub has_screenshot: bool,
    /// At least one recording (gif/mp4/mov/webm) in evidence artifacts.
    pub has_recording: bool,
}

/// Query task_artifacts to compute artifact gates for a single task.
pub(crate) async fn compute_artifact_gates(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    reopen_seq: i64,
    reopened_at: Option<&str>,
) -> ArtifactGates {
    let artifacts = crate::io::queries::artifacts::list_for_task(pool, task_id)
        .await
        .unwrap_or_default();

    let evidence_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| a.artifact_type == crate::ArtifactType::Evidence)
        .collect();
    let summary_artifacts: Vec<_> = artifacts
        .iter()
        .filter(|a| a.artifact_type == crate::ArtifactType::WorkSummary)
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

    // Typed evidence gates use only fresh artifacts (same threshold as evidence_fresh).
    let fresh_evidence: Vec<_> = if reopen_seq == 0 || reopened_at.is_none() {
        evidence_artifacts.clone()
    } else {
        let threshold = reopened_at.unwrap_or("");
        evidence_artifacts
            .iter()
            .filter(|a| a.created_at.as_str() > threshold)
            .cloned()
            .collect()
    };
    let has_screenshot = fresh_evidence.iter().any(|a| {
        a.media
            .iter()
            .any(|m| SCREENSHOT_EXTS.contains(&m.ext.to_lowercase().as_str()))
    });
    let has_recording = fresh_evidence.iter().any(|a| {
        a.media
            .iter()
            .any(|m| RECORDING_EXTS.contains(&m.ext.to_lowercase().as_str()))
    });

    ArtifactGates {
        has_evidence,
        evidence_fresh,
        has_work_summary,
        work_summary_fresh,
        has_screenshot,
        has_recording,
    }
}
