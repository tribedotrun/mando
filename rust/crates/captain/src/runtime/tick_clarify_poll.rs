//! Async clarifier polling — checks for completed clarifier sessions.
//!
//! Clarifier CC sessions run as detached tokio tasks. On each tick, this
//! module polls the stream files for results and applies them.

use std::collections::HashMap;

use api_types::TimelineEventPayload;

use crate::{ItemStatus, Task};
use settings::config::workflow::CaptainWorkflow;

use super::tick_clarify_apply::apply_clarifier_result;
use super::{clarifier, credential_rate_limit, notify::Notifier, timeline_emit};
use crate::service::lifecycle;

/// Tick-path persist failures are transient: the next tick re-polls the same
/// session and retries the apply, so we log and move on instead of
/// propagating. The HTTP path (`routes_clarifier.rs`) surfaces the same error
/// as a 500 to the caller.
fn log_apply_err(item_id: i64, res: anyhow::Result<()>) {
    if let Err(e) = res {
        tracing::error!(
            module = "captain",
            id = item_id,
            error = %e,
            "clarifier apply failed to persist, will retry next tick"
        );
    }
}

#[tracing::instrument(skip_all)]
pub(super) async fn poll_clarifying_items(
    items: &mut [Task],
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

        // Presence was checked above — the early `continue` guards this.
        let Some(session_id) = item.session_ids.clarifier.clone() else {
            continue;
        };
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
                    global_infra::best_effort!(
                        timeline_emit::emit_rate_limited(item, pool).await,
                        "tick_clarify_poll: timeline_emit::emit_rate_limited(item, pool).await"
                    );
                    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::New) {
                        tracing::error!(
                            module = "captain",
                            item_id = item.id,
                            error = %e,
                            "illegal clarifier retry transition"
                        );
                        continue;
                    }
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
                        timestamp: global_types::now_rfc3339(),
                        actor: "captain".to_string(),
                        summary: format!(
                            "Clarifier failed {count} times — escalating to captain review"
                        ),
                        data: TimelineEventPayload::CaptainReviewClarifierFail {
                            trigger: "clarifier_fail".to_string(),
                            fail_count: count,
                        },
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
                    log_apply_err(
                        item.id,
                        apply_clarifier_result(
                            item,
                            parsed,
                            &session_id,
                            notifier,
                            resource_limits,
                            pool,
                        )
                        .await,
                    );
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

            log_apply_err(
                item.id,
                apply_clarifier_result(item, parsed, &session_id, notifier, resource_limits, pool)
                    .await,
            );
            continue;
        }

        // No `type: "result"` entry in the stream. If the session is already
        // finished (stopped/failed/timeout), try extracting the answer from the
        // last assistant message instead of waiting for the full timeout.
        if global_claude::is_session_finished(&session_id) {
            if let Some(assistant_text) = global_claude::get_last_assistant_text(&stream_path) {
                let mut parsed = clarifier::parse_clarifier_response(&assistant_text, &item.title);
                parsed.session_id = Some(session_id.to_string());
                log_apply_err(
                    item.id,
                    apply_clarifier_result(
                        item,
                        parsed,
                        &session_id,
                        notifier,
                        resource_limits,
                        pool,
                    )
                    .await,
                );
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
                global_infra::best_effort!(
                    timeline_emit::emit_rate_limited(item, pool).await,
                    "tick_clarify_poll: timeline_emit::emit_rate_limited(item, pool).await"
                );
                if let Err(e) = lifecycle::apply_transition(item, ItemStatus::New) {
                    tracing::error!(
                        module = "captain",
                        item_id = item.id,
                        error = %e,
                        "illegal clarifier timeout retry transition"
                    );
                    continue;
                }
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
                    timestamp: global_types::now_rfc3339(),
                    actor: "captain".to_string(),
                    summary: format!(
                        "Clarifier timed out {count} times — escalating to captain review"
                    ),
                    data: TimelineEventPayload::CaptainReviewClarifierFail {
                        trigger: "clarifier_timeout".to_string(),
                        fail_count: count,
                    },
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
