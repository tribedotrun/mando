//! Clarifier timeout enforcement — escalate stale NeedsClarification items.

use crate::{ItemStatus, Task};
use settings::config::workflow::CaptainWorkflow;

use super::dashboard::truncate_utf8;
use super::notify::Notifier;

/// Check NeedsClarification items for timeout and escalate stale ones.
///
/// Items sitting in NeedsClarification longer than `needs_clarification_timeout_s`
/// (default 24h) are escalated to CaptainReviewing with a ClarifierFail trigger.
/// This is separate from `clarifier_timeout_s` (CC session timeout, default 300s).
pub(super) async fn check_clarifier_timeouts(
    items: &mut [Task],
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let timeout = workflow.agent.needs_clarification_timeout_s;
    let timeout_s = timeout.as_secs();

    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::NeedsClarification)
    {
        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > timeout_s
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        last_activity_at = %ts,
                        error = %e,
                        "unparseable last_activity_at on needs-clarification item; skipping this tick"
                    );
                    continue;
                }
            },
            None => {
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    "needs-clarification item has no last_activity_at; skipping this tick"
                );
                continue;
            }
        };

        if !is_timed_out {
            continue;
        }

        tracing::warn!(
            module = "captain",
            item_id = item.id,
            title = %truncate_utf8(&item.title, 60),
            timeout_s = timeout_s,
            "NeedsClarification item timed out — escalating"
        );

        let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
        super::action_contract::reset_review_retry(item, crate::ReviewTrigger::ClarifierFail);

        let event = crate::TimelineEvent {
            event_type: crate::TimelineEventType::ClarifyQuestion,
            timestamp: global_types::now_rfc3339(),
            actor: "captain".to_string(),
            summary: format!(
                "Clarification timed out after {}s — escalating to captain review",
                timeout_s
            ),
            data: serde_json::json!({"timeout_s": timeout_s}),
        };
        match crate::io::queries::tasks::persist_status_transition(
            pool,
            item,
            snap.status.as_str(),
            &event,
        )
        .await
        {
            Ok(true) => {
                let msg = format!(
                    "\u{23f0} Clarification timed out for <b>{}</b> ({}s) — escalating",
                    global_infra::html::escape_html(&item.title),
                    timeout_s,
                );
                notifier.high(&msg).await;
            }
            Ok(false) => {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "clarify timeout already applied"
                );
            }
            Err(e) => {
                snap.restore(item);
                tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for clarify timeout");
            }
        }
    }
}
