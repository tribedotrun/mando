//! Transition into `CaptainMerging` after a high-confidence auto-merge
//! triage verdict. Extracted from the main `auto_merge_triage` module to
//! keep that file under the project's 500-line limit.

use tracing::{info, warn};

use mando_config::settings::Config;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::auto_merge_triage_gate::TriageResult;
use super::notify::Notifier;

/// Transition the item to CaptainMerging after a high-confidence verdict.
/// Reverts the in-memory status if the persist guard rejects the change
/// (e.g. another tick already moved the item) so the next tick can re-evaluate.
pub(crate) async fn transition_to_merging(
    item: &mut Task,
    result: &TriageResult,
    config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let pr_num = item.pr_number.unwrap_or(0);
    let repo = item
        .github_repo
        .clone()
        .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config))
        .unwrap_or_default();
    let pr_url = format!("https://github.com/{repo}/pull/{pr_num}");

    let prev_status = item.status;
    item.status = ItemStatus::CaptainMerging;
    item.session_ids.merge = None;
    item.merge_fail_count = 0;
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let event = mando_types::timeline::TimelineEvent {
        event_type: TimelineEventType::CaptainMergeStarted,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "Auto-merge triage passed -- starting merge".to_string(),
        data: serde_json::json!({
            "pr": &pr_url,
            "source": "auto_merge_triage",
        }),
    };

    match mando_db::queries::tasks::persist_status_transition(
        pool,
        item,
        prev_status.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            let title = mando_shared::telegram_format::escape_html(&item.title);
            notifier
                .normal(&format!(
                    "\u{2705} Auto-merge triage passed for <b>{title}</b> -- merging"
                ))
                .await;
            info!(
                module = "captain",
                item_id = item.id,
                confidence = %result.confidence,
                "auto-merge triage passed, transitioning to CaptainMerging"
            );
        }
        Ok(false) => {
            item.status = prev_status;
            info!(
                module = "captain",
                item_id = item.id,
                "auto-merge triage transition already applied"
            );
        }
        Err(e) => {
            item.status = prev_status;
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto-merge triage transition"
            );
        }
    }
}
