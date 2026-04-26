//! Applies a parsed clarifier result to a task.

use std::collections::HashMap;

use crate::service::lifecycle;
use crate::{ItemStatus, Task};
use api_types::{ClarifierQuestionPayload, TimelineEventPayload};

use super::{clarifier, notify::Notifier};
use crate::runtime::dashboard::truncate_utf8;

/// Applies a parsed clarifier result to the in-memory task and persists the
/// transition to the DB.
///
/// Returns `Err` only when the DB write fails. The HTTP clarify endpoint
/// surfaces this as a 500; the captain tick path logs and retries next cycle.
/// Timeline/notifier/workbench side-effects stay best-effort (logged only) so
/// the caller can still act on the in-memory state the LLM produced.
#[tracing::instrument(skip_all)]
pub async fn apply_clarifier_result(
    item: &mut Task,
    result: clarifier::ClarifierResult,
    session_id: &str,
    notifier: &Notifier,
    resource_limits: &HashMap<String, usize>,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<()> {
    match result.status {
        clarifier::ClarifierStatus::Ready => {
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }
            let context_trimmed: String = result
                .context
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            if context_trimmed.len() < 20 {
                tracing::warn!(
                    module = "captain",
                    title = %truncate_utf8(&item.title, 60),
                    context_len = context_trimmed.len(),
                    "clarifier returned trivial context (<20 chars), escalating"
                );
                super::action_contract::reset_review_retry(
                    item,
                    crate::ReviewTrigger::ClarifierFail,
                );
                item.context = Some(result.context);

                crate::io::queries::tasks::persist_clarify_result(pool, item)
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            module = "captain",
                            id = item.id,
                            error = %e,
                            "failed to persist trivial-context escalation"
                        );
                    })?;

                notifier
                    .high(&format!(
                        "\u{26a0}\u{fe0f} Clarifier returned trivial output for <b>{}</b>",
                        global_infra::html::escape_html(&item.title),
                    ))
                    .await;
            } else {
                lifecycle::apply_transition(item, ItemStatus::Queued)?;
                item.clarifier_fail_count = 0;
                item.context = Some(result.context);
                if let Some(title) = result.generated_title {
                    if !title.is_empty() {
                        item.title = title;
                    }
                }
                if let Some(no_pr) = result.no_pr {
                    item.no_pr = no_pr;
                }
                if let Some(is_bug_fix) = result.is_bug_fix {
                    item.is_bug_fix = is_bug_fix;
                }
                if let Some(ref resource) = result.resource {
                    let is_known = resource == "cc" || resource_limits.contains_key(resource);
                    if is_known {
                        item.resource = Some(resource.clone());
                    } else {
                        tracing::warn!(
                            module = "captain",
                            resource = %resource,
                            title = %truncate_utf8(&item.title, 60),
                            "clarifier returned unknown resource -- ignoring"
                        );
                    }
                }

                crate::io::queries::tasks::persist_clarify_result(pool, item)
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            module = "captain",
                            id = item.id,
                            error = %e,
                            "failed to persist clarify result"
                        );
                    })?;

                // Propagate the clarified title to the parent workbench.
                {
                    let wb_id = item.workbench_id;
                    if let Err(e) =
                        crate::io::queries::workbenches::update_title(pool, wb_id, &item.title)
                            .await
                    {
                        tracing::warn!(
                            module = "captain",
                            workbench_id = wb_id,
                            error = %e,
                            "failed to propagate title to workbench"
                        );
                    } else {
                        notifier
                            .clone_bus()
                            .send(global_bus::BusPayload::Workbenches(None));
                    }
                }

                global_infra::best_effort!(super::timeline_emit::emit_for_task(
                    item,
                    "Clarification complete, ready for work",
                    TimelineEventPayload::ClarifyResolved {
                        session_id: session_id.to_string(),
                    },
                    pool,
                )
                .await, "tick_clarify_apply: super::timeline_emit::emit_for_task( item, 'Clarification co");
                tracing::info!(
                    module = "captain",
                    title = %truncate_utf8(&item.title, 60),
                    "clarified, now ready"
                );
            }
        }
        clarifier::ClarifierStatus::Answered => {
            lifecycle::apply_transition(item, ItemStatus::CompletedNoPr)?;
            item.no_pr = true;
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }
            if let Some(title) = result.generated_title {
                if !title.is_empty() {
                    item.title = title;
                }
            }
            if let Some(is_bug_fix) = result.is_bug_fix {
                item.is_bug_fix = is_bug_fix;
            }

            crate::io::queries::tasks::persist_clarify_result(pool, item)
                .await
                .inspect_err(|e| {
                    tracing::error!(
                        module = "captain",
                        id = item.id,
                        error = %e,
                        "failed to persist clarify result"
                    );
                })?;

            global_infra::best_effort!(
                super::timeline_emit::emit_for_task(
                    item,
                    "Clarifier answered directly, no work needed",
                    TimelineEventPayload::ClarifierCompletedNoPr {
                        session_id: session_id.to_string(),
                    },
                    pool,
                )
                .await,
                "tick_clarify_apply: super::timeline_emit::emit_for_task( item, 'Clarifier answer"
            );
            let answer_preview = item
                .context
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(500)
                .collect::<String>();
            notifier
                .high(&format!(
                    "\u{2705} Answered: <b>{}</b>\n{}",
                    global_infra::html::escape_html(&item.title),
                    global_infra::html::escape_html(&answer_preview),
                ))
                .await;

            tracing::info!(
                module = "captain",
                title = %truncate_utf8(&item.title, 60),
                "clarifier answered directly, completed without worker"
            );
        }
        clarifier::ClarifierStatus::Clarifying => {
            lifecycle::apply_transition(item, ItemStatus::NeedsClarification)?;
            item.last_activity_at = Some(global_types::now_rfc3339());
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }
            if let Some(is_bug_fix) = result.is_bug_fix {
                item.is_bug_fix = is_bug_fix;
            }

            crate::io::queries::tasks::persist_clarify_result(pool, item)
                .await
                .inspect_err(|e| {
                    tracing::error!(
                        module = "captain",
                        id = item.id,
                        error = %e,
                        "failed to persist clarify result"
                    );
                })?;

            global_infra::best_effort!(
                super::timeline_emit::emit_for_task(
                    item,
                    "Needs clarification",
                    TimelineEventPayload::ClarifyQuestion {
                        session_id: result.session_id.clone().unwrap_or_default(),
                        questions: result
                            .questions
                            .as_ref()
                            .map(|qs| {
                                qs.iter()
                                    .map(|q| ClarifierQuestionPayload {
                                        question: q.question.clone(),
                                        answer: q.answer.clone(),
                                        self_answered: q.self_answered,
                                        category: q.category.clone(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                    pool,
                )
                .await,
                "tick_clarify_apply: super::timeline_emit::emit_for_task( item, 'Needs clarificat"
            );
            if let Some(ref questions) = result.questions {
                let text = clarifier::format_questions_text(questions);
                let msg = format!(
                    "\u{2753} Needs clarification: <b>{}</b>\n{}",
                    global_infra::html::escape_html(&item.title),
                    global_infra::html::escape_html(&text),
                );
                notifier
                    .notify_typed(
                        &msg,
                        api_types::NotifyLevel::High,
                        api_types::NotificationKind::NeedsClarification {
                            item_id: item.id.to_string(),
                            questions: Some(text),
                        },
                        Some(&item.id.to_string()),
                    )
                    .await;
            }
        }
        clarifier::ClarifierStatus::Escalate => {
            super::action_contract::reset_review_retry(item, crate::ReviewTrigger::ClarifierFail);
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }

            crate::io::queries::tasks::persist_clarify_result(pool, item)
                .await
                .inspect_err(|e| {
                    tracing::error!(
                        module = "captain",
                        id = item.id,
                        error = %e,
                        "failed to persist clarify result"
                    );
                })?;

            global_infra::best_effort!(
                super::timeline_emit::emit_for_task(
                    item,
                    "Needs human input",
                    TimelineEventPayload::ClarifyQuestion {
                        session_id: result.session_id.clone().unwrap_or_default(),
                        questions: result
                            .questions
                            .as_ref()
                            .map(|qs| {
                                qs.iter()
                                    .map(|q| ClarifierQuestionPayload {
                                        question: q.question.clone(),
                                        answer: q.answer.clone(),
                                        self_answered: q.self_answered,
                                        category: q.category.clone(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                    pool,
                )
                .await,
                "tick_clarify_apply: super::timeline_emit::emit_for_task( item, 'Needs human inpu"
            );
            if let Some(ref questions) = result.questions {
                let text = clarifier::format_questions_text(questions);
                let msg = format!(
                    "\u{2753} Needs human input: <b>{}</b>\n{}",
                    global_infra::html::escape_html(&item.title),
                    global_infra::html::escape_html(&text),
                );
                notifier
                    .notify_typed(
                        &msg,
                        api_types::NotifyLevel::High,
                        api_types::NotificationKind::Escalated {
                            item_id: item.id.to_string(),
                            summary: Some(text),
                        },
                        Some(&item.id.to_string()),
                    )
                    .await;
            }
        }
    }
    Ok(())
}
