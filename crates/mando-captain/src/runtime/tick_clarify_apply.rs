//! Applies a parsed clarifier result to a task.

use std::collections::HashMap;

use mando_config::settings::Config;
use mando_types::task::{ItemStatus, Task};

use super::{clarifier, notify::Notifier};
use crate::runtime::dashboard::truncate_utf8;

#[allow(clippy::too_many_arguments)]
pub(super) async fn apply_clarifier_result(
    item: &mut Task,
    result: clarifier::ClarifierResult,
    session_id: &str,
    config: &Config,
    notifier: &Notifier,
    resource_limits: &HashMap<String, usize>,
    pool: &sqlx::SqlitePool,
) {
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
                    mando_types::task::ReviewTrigger::ClarifierFail,
                );
                item.context = Some(result.context);
                notifier
                    .high(&format!(
                        "\u{26a0}\u{fe0f} Clarifier returned trivial output for <b>{}</b>",
                        mando_shared::telegram_format::escape_html(&item.title),
                    ))
                    .await;
            } else {
                item.status = ItemStatus::Queued;
                item.clarifier_fail_count = 0;
                item.context = Some(result.context);
                if let Some(title) = result.generated_title {
                    if !title.is_empty() {
                        item.title = title;
                    }
                }
                if let Some(repo) = result.repo.filter(|r| !r.trim().is_empty()) {
                    match mando_config::resolve_project_config(Some(&repo), config) {
                        Some((_, pc)) => {
                            if let Ok(id) = mando_db::queries::projects::upsert(
                                pool,
                                &pc.name,
                                &pc.path,
                                pc.github_repo.as_deref(),
                            )
                            .await
                            {
                                item.project_id = id;
                            }
                            item.project = pc.name.clone();
                        }
                        None => {
                            tracing::error!(
                                module = "captain",
                                repo = %repo,
                                title = %truncate_utf8(&item.title, 60),
                                "clarifier repo passed schema but fails resolve -- escalating"
                            );
                            super::action_contract::reset_review_retry(
                                item,
                                mando_types::task::ReviewTrigger::ClarifierFail,
                            );
                            return;
                        }
                    }
                }
                if let Some(no_pr) = result.no_pr {
                    item.no_pr = no_pr;
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

                if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                    tracing::error!(
                        module = "captain",
                        id = item.id,
                        error = %e,
                        "failed to persist clarify result"
                    );
                }

                // Propagate the clarified title to the parent workbench.
                if let Some(wb_id) = item.workbench_id {
                    if let Err(e) =
                        mando_db::queries::workbenches::update_title(pool, wb_id, &item.title).await
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
                            .send(mando_types::BusEvent::Workbenches, None);
                    }
                }

                let _ = super::timeline_emit::emit_for_task(
                    item,
                    mando_types::timeline::TimelineEventType::ClarifyResolved,
                    "Clarification complete, ready for work",
                    serde_json::json!({"session_id": session_id}),
                    pool,
                )
                .await;

                tracing::info!(
                    module = "captain",
                    title = %truncate_utf8(&item.title, 60),
                    "clarified, now ready"
                );
            }
        }
        clarifier::ClarifierStatus::Clarifying => {
            item.status = ItemStatus::NeedsClarification;
            item.last_activity_at = Some(mando_types::now_rfc3339());
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }

            if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "failed to persist clarify result"
                );
            }

            let _ = super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::ClarifyQuestion,
                "Needs clarification",
                serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                pool,
            )
            .await;

            if let Some(ref questions) = result.questions {
                let text = clarifier::format_questions_text(questions);
                let msg = format!(
                    "\u{2753} Needs clarification: <b>{}</b>\n{}",
                    mando_shared::telegram_format::escape_html(&item.title),
                    mando_shared::telegram_format::escape_html(&text),
                );
                notifier
                    .notify_typed(
                        &msg,
                        mando_types::notify::NotifyLevel::High,
                        mando_types::events::NotificationKind::NeedsClarification {
                            item_id: item.id.to_string(),
                            questions: Some(text),
                        },
                        Some(&item.id.to_string()),
                    )
                    .await;
            }
        }
        clarifier::ClarifierStatus::Escalate => {
            super::action_contract::reset_review_retry(
                item,
                mando_types::task::ReviewTrigger::ClarifierFail,
            );
            item.context = Some(result.context);
            if let Some(ref sid) = result.session_id {
                item.session_ids.clarifier = Some(sid.clone());
            }

            if let Err(e) = mando_db::queries::tasks::persist_clarify_result(pool, item).await {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "failed to persist clarify result"
                );
            }

            let _ = super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::ClarifyQuestion,
                "Needs human input",
                serde_json::json!({"session_id": result.session_id, "questions": result.questions}),
                pool,
            )
            .await;

            if let Some(ref questions) = result.questions {
                let text = clarifier::format_questions_text(questions);
                let msg = format!(
                    "\u{2753} Needs human input: <b>{}</b>\n{}",
                    mando_shared::telegram_format::escape_html(&item.title),
                    mando_shared::telegram_format::escape_html(&text),
                );
                notifier
                    .notify_typed(
                        &msg,
                        mando_types::notify::NotifyLevel::High,
                        mando_types::events::NotificationKind::Escalated {
                            item_id: item.id.to_string(),
                            summary: Some(text),
                        },
                        Some(&item.id.to_string()),
                    )
                    .await;
            }
        }
    }
}
