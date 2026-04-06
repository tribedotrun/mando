//! Async clarifier polling — checks for completed clarifier sessions.
//!
//! Clarifier CC sessions run as detached tokio tasks. On each tick, this
//! module polls the stream files for results and applies them.

use std::collections::HashMap;

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use super::{clarifier, notify::Notifier, rate_limit_cooldown, timeline_emit};
use crate::runtime::dashboard::truncate_utf8;

pub(super) async fn poll_clarifying_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    rate_limited: bool,
    resource_limits: &HashMap<String, usize>,
) {
    let clarifier_timeout = workflow.agent.clarifier_timeout_s;
    let max_clarifier_retries = workflow.agent.max_clarifier_retries as i64;

    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::Clarifying)
    {
        let has_session = item
            .session_ids
            .clarifier
            .as_deref()
            .is_some_and(|s| !s.is_empty());

        if !has_session {
            // No session — this item needs a clarifier session spawned.
            // But spawning happens in dispatch_clarify, not here.
            // Skip — dispatch phase will handle it.
            continue;
        }

        let session_id = item.session_ids.clarifier.clone().unwrap();
        let stream_path = mando_config::stream_path_for_session(&session_id);

        // Check for error result first (like check_review_failed).
        if let Some(result) = mando_cc::get_stream_result(&stream_path) {
            if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                let error_msg = result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("clarifier CC process failed")
                    .to_string();

                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    %session_id,
                    %error_msg,
                    "async clarifier session failed"
                );

                // Check rate limiting — revert to New so dispatch_clarify
                // (not dispatch_reclarify) handles the retry with the correct
                // initial clarification prompt.
                if rate_limit_cooldown::check_and_activate_from_stream(&session_id) {
                    let _ = timeline_emit::emit_rate_limited(item, pool).await;
                    item.status = ItemStatus::New;
                    item.session_ids.clarifier = None;
                    continue;
                }

                // Revert: mark session failed, revert status to New
                super::dispatch_redispatch::revert_clarifier_start(
                    item,
                    &session_id,
                    &anyhow::anyhow!("{}", error_msg),
                    pool,
                )
                .await;

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
                        "clarifier failed {} times — escalating",
                        count
                    );
                    notifier
                        .high(&format!(
                            "\u{274c} Clarifier failed {} times for <b>{}</b> — needs human",
                            count,
                            mando_shared::telegram_format::escape_html(&item.title),
                        ))
                        .await;
                } else {
                    tracing::warn!(
                        module = "captain",
                        title = %item.title,
                        attempt = count,
                        "clarifier failed, will retry on next tick"
                    );
                }
                continue;
            }

            // Success result — parse the ClarifierResult from structured_output.
            let text = result
                .get("structured_output")
                .filter(|v| !v.is_null())
                .map(|v| v.to_string())
                .or_else(|| {
                    result
                        .get("result")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .unwrap_or_default();

            if text.is_empty() {
                // Session completed but produced no output — treat as an error.
                // Try last assistant text as a fallback.
                if let Some(assistant_text) = mando_cc::get_last_assistant_text(&stream_path) {
                    let parsed = clarifier::parse_clarifier_response(&assistant_text, &item.title);
                    apply_clarifier_result(
                        item,
                        parsed,
                        &session_id,
                        config,
                        notifier,
                        resource_limits,
                        pool,
                    )
                    .await;
                } else {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        "clarifier completed but produced no output — escalating"
                    );
                    super::action_contract::reset_review_retry(
                        item,
                        mando_types::task::ReviewTrigger::ClarifierFail,
                    );
                }
                continue;
            }

            let mut parsed = clarifier::parse_clarifier_response(&text, &item.title);
            parsed.session_id = Some(session_id.to_string());

            // Validate repo name. If invalid, the spawned task should have
            // already retried. If we still see an invalid repo here, escalate.
            if let Some(ref repo) = parsed.repo {
                if !repo.trim().is_empty()
                    && mando_config::resolve_project_config(Some(repo), config).is_none()
                {
                    tracing::warn!(
                        module = "captain",
                        repo = %repo,
                        title = %truncate_utf8(&item.title, 60),
                        "clarifier returned unresolvable repo after async run — escalating"
                    );
                    super::action_contract::reset_review_retry(
                        item,
                        mando_types::task::ReviewTrigger::ClarifierFail,
                    );
                    continue;
                }
            }

            apply_clarifier_result(
                item,
                parsed,
                &session_id,
                config,
                notifier,
                resource_limits,
                pool,
            )
            .await;
            continue;
        }

        // No result yet — check timeout.
        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > clarifier_timeout.as_secs()
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        last_activity_at = %ts,
                        error = %e,
                        "unparseable last_activity_at on clarifying item"
                    );
                    continue;
                }
            },
            None => {
                // No timestamp — likely a crash between persist_clarify_start
                // (which only writes status/session_ids) and tick-end merge
                // (which writes last_activity_at). Stamp it now so timeout
                // detection works on the next tick instead of skipping forever.
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    "clarifying item has no last_activity_at — stamping now"
                );
                item.last_activity_at = Some(mando_types::now_rfc3339());
                continue;
            }
        };

        if is_timed_out {
            if rate_limited || rate_limit_cooldown::check_and_activate_from_stream(&session_id) {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "clarifier timeout during rate limit — not counting against retry budget"
                );
                let _ = timeline_emit::emit_rate_limited(item, pool).await;
                item.status = ItemStatus::New;
                item.session_ids.clarifier = None;
                continue;
            }

            tracing::warn!(
                module = "captain",
                item_id = item.id,
                "clarifier session timed out"
            );

            super::dispatch_redispatch::revert_clarifier_start(
                item,
                &session_id,
                &anyhow::anyhow!("clarifier session timed out"),
                pool,
            )
            .await;

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
                    "clarifier timed out {} times — escalating",
                    count
                );
                notifier
                    .high(&format!(
                        "\u{274c} Clarifier timed out {} times for <b>{}</b> — escalating",
                        count,
                        mando_shared::telegram_format::escape_html(&item.title),
                    ))
                    .await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_clarifier_result(
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
                            item.project = Some(pc.name.clone());
                        }
                        None => {
                            tracing::error!(
                                module = "captain",
                                repo = %repo,
                                title = %truncate_utf8(&item.title, 60),
                                "clarifier repo passed schema but fails resolve — escalating"
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
                            "clarifier returned unknown resource — ignoring"
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
