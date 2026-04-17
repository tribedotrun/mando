//! Async clarifier polling — checks for completed clarifier sessions.
//!
//! Clarifier CC sessions run as detached tokio tasks. On each tick, this
//! module polls the stream files for results and applies them.

use std::collections::HashMap;

use crate::{ItemStatus, Task};
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use super::tick_clarify_apply::apply_clarifier_result;
use super::{clarifier, credential_rate_limit, notify::Notifier, timeline_emit};
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
        let stream_path = global_infra::paths::stream_path_for_session(&session_id);

        // Check for error result first (like check_review_failed).
        if let Some(result) = global_claude::get_stream_result(&stream_path) {
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
                if credential_rate_limit::check_and_activate_from_stream(pool, &session_id).await {
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
                    let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
                    super::action_contract::reset_review_retry(
                        item,
                        crate::ReviewTrigger::ClarifierFail,
                    );
                    let event = crate::TimelineEvent {
                        event_type: crate::TimelineEventType::CaptainReviewStarted,
                        timestamp: global_types::now_rfc3339(),
                        actor: "captain".to_string(),
                        summary: format!(
                            "Clarifier failed {count} times — escalating to captain review"
                        ),
                        data: serde_json::json!({"fail_count": count, "trigger": "clarifier_fail"}),
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
                                    global_infra::html::escape_html(&item.title),
                                ))
                                .await;
                        }
                        Ok(false) => {
                            tracing::info!(
                                module = "captain",
                                "clarifier escalation already applied"
                            );
                        }
                        Err(e) => {
                            snap.restore(item);
                            tracing::error!(module = "captain", error = %e, "persist failed for clarifier escalation");
                        }
                    }
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
                if let Some(assistant_text) = global_claude::get_last_assistant_text(&stream_path) {
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
                        crate::ReviewTrigger::ClarifierFail,
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
                    && settings::config::resolve_project_config(Some(repo), config).is_none()
                {
                    tracing::warn!(
                        module = "captain",
                        repo = %repo,
                        title = %truncate_utf8(&item.title, 60),
                        "clarifier returned unresolvable repo after async run — escalating"
                    );
                    super::action_contract::reset_review_retry(
                        item,
                        crate::ReviewTrigger::ClarifierFail,
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

        // No `type: "result"` entry in the stream. If the session is already
        // finished (stopped/failed/timeout), try extracting the answer from the
        // last assistant message instead of waiting for the full timeout.
        if global_claude::is_session_finished(&session_id) {
            if let Some(assistant_text) = global_claude::get_last_assistant_text(&stream_path) {
                let mut parsed = clarifier::parse_clarifier_response(&assistant_text, &item.title);
                parsed.session_id = Some(session_id.to_string());
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
            // Session finished but produced nothing useful — treat as error.
            tracing::warn!(
                module = "captain",
                item_id = item.id,
                %session_id,
                "clarifier session finished without result or assistant text"
            );
            super::dispatch_redispatch::revert_clarifier_start(
                item,
                &session_id,
                &anyhow::anyhow!("session finished without usable output"),
                pool,
            )
            .await;
            let count = item.clarifier_fail_count + 1;
            item.clarifier_fail_count = count;
            if count >= max_clarifier_retries {
                super::action_contract::reset_review_retry(
                    item,
                    crate::ReviewTrigger::ClarifierFail,
                );
            }
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
                item.last_activity_at = Some(global_types::now_rfc3339());
                continue;
            }
        };

        if is_timed_out {
            if rate_limited
                || credential_rate_limit::check_and_activate_from_stream(pool, &session_id).await
            {
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
                let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
                super::action_contract::reset_review_retry(
                    item,
                    crate::ReviewTrigger::ClarifierFail,
                );
                let event = crate::TimelineEvent {
                    event_type: crate::TimelineEventType::CaptainReviewStarted,
                    timestamp: global_types::now_rfc3339(),
                    actor: "captain".to_string(),
                    summary: format!(
                        "Clarifier timed out {count} times — escalating to captain review"
                    ),
                    data: serde_json::json!({"fail_count": count, "trigger": "clarifier_timeout"}),
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
                                global_infra::html::escape_html(&item.title),
                            ))
                            .await;
                    }
                    Ok(false) => {
                        tracing::info!(
                            module = "captain",
                            "clarifier timeout escalation already applied"
                        );
                    }
                    Err(e) => {
                        snap.restore(item);
                        tracing::error!(module = "captain", error = %e, "persist failed for clarifier timeout escalation");
                    }
                }
            }
        }
    }
}
