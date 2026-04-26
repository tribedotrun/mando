//! Evidence-listing computation for the captain review prompt.
//!
//! Walks fresh evidence artifacts on a task, builds the human-readable
//! `evidence_files` listing the reviewer LLM sees, and computes the typed
//! gate flags it routes on (`has_screenshot`, `has_recording`,
//! `has_before_fix`, `has_after_fix`, `has_cannot_reproduce`).
//!
//! Lifted out of `captain_review.rs` to keep that file focused on the
//! review-spawn orchestration.

use crate::Task;

use super::review_phase_artifacts::{RECORDING_EXTS, SCREENSHOT_EXTS};

#[derive(Debug, Default)]
pub(crate) struct EvidenceListing {
    pub listing: String,
    pub work_summary: String,
    pub has_screenshot: bool,
    pub has_recording: bool,
    pub has_before_fix: bool,
    pub has_after_fix: bool,
    pub has_cannot_reproduce: bool,
}

/// Load fresh evidence + work summary for `item` and produce the listing
/// + typed-gate flags consumed by the captain review prompt.
#[tracing::instrument(skip_all, fields(task_id = item.id))]
pub(crate) async fn compute_evidence_listing(
    pool: &sqlx::SqlitePool,
    item: &Task,
) -> EvidenceListing {
    let artifacts = crate::io::queries::artifacts::list_for_task(pool, item.id)
        .await
        .unwrap_or_default();
    let data_dir = global_types::data_dir();
    let freshness_threshold = item.reopened_at.as_deref().unwrap_or("");
    let is_reopened = item.reopen_seq > 0 && item.reopened_at.is_some();

    let mut out = EvidenceListing::default();
    for artifact in &artifacts {
        if artifact.artifact_type != crate::ArtifactType::Evidence {
            continue;
        }
        let is_fresh = !is_reopened || artifact.created_at.as_str() > freshness_threshold;
        for media in &artifact.media {
            let ext_lower = media.ext.to_lowercase();
            if is_fresh && SCREENSHOT_EXTS.contains(&ext_lower.as_str()) {
                out.has_screenshot = true;
            }
            if is_fresh && RECORDING_EXTS.contains(&ext_lower.as_str()) {
                out.has_recording = true;
            }
            if is_fresh {
                match media.kind {
                    Some(crate::EvidenceKind::BeforeFix) => out.has_before_fix = true,
                    Some(crate::EvidenceKind::AfterFix) => out.has_after_fix = true,
                    Some(crate::EvidenceKind::CannotReproduce) => out.has_cannot_reproduce = true,
                    Some(crate::EvidenceKind::Other) | None => {}
                }
            }
            if let Some(ref local) = media.local_path {
                let caption = media.caption.as_deref().unwrap_or("(no caption)");
                let kind_label = match media.kind {
                    Some(crate::EvidenceKind::BeforeFix) => " [before_fix]",
                    Some(crate::EvidenceKind::AfterFix) => " [after_fix]",
                    Some(crate::EvidenceKind::CannotReproduce) => " [cannot_reproduce]",
                    Some(crate::EvidenceKind::Other) | None => "",
                };
                out.listing.push_str(&format!(
                    "- {} ({}){}\n",
                    data_dir.join(local).display(),
                    caption,
                    kind_label,
                ));
            }
        }
    }

    out.work_summary = artifacts
        .iter()
        .rfind(|a| a.artifact_type == crate::ArtifactType::WorkSummary)
        .map(|a| a.content.clone())
        .unwrap_or_default();

    out
}
