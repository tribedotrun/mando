//! Clarifier timeout enforcement — escalate stale NeedsClarification items.

use mando_config::workflow::CaptainWorkflow;
use mando_types::task::ItemStatus;
use mando_types::Task;

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
    let timeout_s = workflow.agent.needs_clarification_timeout_s;

    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::NeedsClarification)
    {
        let is_timed_out = item
            .last_activity_at
            .as_deref()
            .and_then(|ts| {
                time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339).ok()
            })
            .map(|entered| {
                let elapsed = time::OffsetDateTime::now_utc() - entered;
                elapsed.whole_seconds() as u64 > timeout_s
            })
            .unwrap_or(true); // No timestamp = treat as timed out.

        if !is_timed_out {
            continue;
        }

        tracing::warn!(
            module = "captain",
            item_id = item.id,
            title = %&item.title[..item.title.len().min(60)],
            timeout_s = timeout_s,
            "NeedsClarification item timed out — escalating"
        );

        super::action_contract::reset_review_retry(
            item,
            mando_types::task::ReviewTrigger::ClarifierFail,
        );

        super::timeline_emit::emit_for_task(
            item,
            mando_types::timeline::TimelineEventType::ClarifyQuestion,
            &format!(
                "Clarification timed out after {}s — escalating to captain review",
                timeout_s
            ),
            serde_json::json!({"timeout_s": timeout_s}),
            pool,
        )
        .await;

        let msg = format!(
            "\u{23f0} Clarification timed out for <b>{}</b> ({}s) — escalating",
            mando_shared::telegram_format::escape_html(&item.title),
            timeout_s,
        );
        notifier.high(&msg).await;
    }
}
