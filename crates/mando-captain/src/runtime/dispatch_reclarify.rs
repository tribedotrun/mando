//! Re-clarification loop — safety net for items stuck in Clarifying status.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::biz::dispatch_logic;
use crate::runtime::clarifier::{self, ClarifierStatus};

/// Re-clarify items where human answered (Clarifying status).
/// Safety net for items that got stuck or where the inline call failed.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn reclarify_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    alerts: &mut Vec<String>,
    max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
) {
    let clarifying = dispatch_logic::clarifying_items(items);
    for idx in clarifying {
        if dry_run {
            dry_actions.push(format!(
                "would re-clarify '{}'",
                &items[idx].title[..items[idx].title.len().min(60)]
            ));
            continue;
        }

        let hint = "(review the context above and decide if the item is ready)";
        match clarifier::answer_and_reclarify(&items[idx], hint, workflow, config, pool).await {
            Ok(result) => {
                let item = &mut items[idx];
                item.clarifier_fail_count = 0;
                match result.status {
                    ClarifierStatus::Ready => {
                        item.status = ItemStatus::Queued;
                        item.context = Some(result.context);
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }

                        super::timeline_emit::emit_for_task(
                            item,
                            mando_types::timeline::TimelineEventType::ClarifyResolved,
                            "Re-clarification complete, ready for work",
                            serde_json::json!({"session_id": result.session_id}),
                            pool,
                        )
                        .await;

                        tracing::info!(
                            module = "captain",
                            title = %&item.title[..item.title.len().min(60)],
                            "re-clarified, now ready"
                        );
                    }
                    ClarifierStatus::Clarifying => {
                        item.status = ItemStatus::NeedsClarification;
                        item.context = Some(result.context);
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }

                        super::timeline_emit::emit_for_task(
                            item,
                            mando_types::timeline::TimelineEventType::ClarifyQuestion,
                            "Still needs clarification",
                            serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                            pool,
                        )
                        .await;
                    }
                    ClarifierStatus::Escalate => {
                        super::action_contract::reset_review_retry(
                            item,
                            mando_types::task::ReviewTrigger::ClarifierFail,
                        );
                        item.context = Some(result.context);
                        if let Some(ref sid) = result.session_id {
                            item.session_ids.clarifier = Some(sid.clone());
                        }
                    }
                }
            }
            Err(e) => {
                let item = &mut items[idx];
                let count = item.clarifier_fail_count + 1;
                item.clarifier_fail_count = count;

                if count >= max_clarifier_retries {
                    super::action_contract::reset_review_retry(
                        item,
                        mando_types::task::ReviewTrigger::ClarifierFail,
                    );
                    tracing::error!(
                        module = "captain",
                        title = %item.title,
                        attempt = count,
                        error = %e,
                        "re-clarification failed 3 times — escalating"
                    );
                    alerts.push(format!(
                        "Re-clarification failed {} times for '{}': {}",
                        count,
                        &item.title[..item.title.len().min(60)],
                        e
                    ));
                } else {
                    tracing::warn!(
                        module = "captain",
                        title = %item.title,
                        attempt = count,
                        error = %e,
                        "re-clarification failed — will retry on next tick"
                    );
                }
            }
        }
    }
}
