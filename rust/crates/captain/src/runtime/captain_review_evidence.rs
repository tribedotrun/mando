//! Evidence-listing computation for the captain review prompt.
//!
//! Walks fresh evidence artifacts on a task and builds the human-readable
//! `evidence_images` listing the reviewer sees, plus the latest work summary.
//!
//! The typed gate flags that used to live here (`has_before_screenshot` and
//! friends) are gone: the rewritten `captain_review` prompt no longer asks
//! the reviewer to check evidence kinds, because
//! `service::deterministic` now enforces them and nudges before a review can
//! fire. `runtime::review_phase_artifacts` owns that computation.
//!
//! Lifted out of `captain_review.rs` to keep that file focused on the
//! review-spawn orchestration.

use crate::Task;

#[derive(Debug, Default)]
pub(crate) struct EvidenceListing {
    pub listing: String,
    pub work_summary: String,
}

/// Load fresh evidence + work summary for `item` and produce the listing
/// consumed by the captain review prompt.
#[tracing::instrument(skip_all, fields(task_id = item.id))]
pub(crate) async fn compute_evidence_listing(
    pool: &sqlx::SqlitePool,
    item: &Task,
) -> EvidenceListing {
    let artifacts = crate::io::queries::artifacts::list_for_task(pool, item.id)
        .await
        .unwrap_or_default();
    let data_dir = global_types::data_dir();

    // Every registered evidence file is listed, fresh or not. Freshness is no
    // longer this function's concern: a task whose evidence predates its
    // latest reopen is nudged by `service::deterministic` and never reaches a
    // review at all.
    let mut out = EvidenceListing::default();
    for artifact in &artifacts {
        if artifact.artifact_type != crate::ArtifactType::Evidence {
            continue;
        }
        for media in &artifact.media {
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
