//! Re-clarification dispatch — spawns re-clarify sessions as background tasks.
//!
//! Safety net for items stuck in Clarifying status where the inline
//! answer_and_reclarify call failed. Sessions run as detached tokio tasks;
//! results are polled by tick_clarify_poll on subsequent ticks.

use std::panic::AssertUnwindSafe;

use crate::{ItemStatus, Task};
use futures::FutureExt;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;
use tokio_util::task::TaskTracker;

use crate::runtime::clarifier;
use crate::runtime::dashboard::truncate_utf8;
use crate::service::dispatch_logic;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(crate) async fn reclarify_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    _alerts: &mut Vec<String>,
    _max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
    task_tracker: &TaskTracker,
) {
    // Only re-clarify items in Clarifying status with no active session.
    // Items with a session_id are handled by tick_clarify_poll.
    // NeedsClarification items are waiting for human input.
    let clarifying: Vec<usize> = dispatch_logic::clarifying_items(items)
        .into_iter()
        .filter(|&idx| {
            let item = &items[idx];
            if item.status != ItemStatus::Clarifying {
                return false;
            }
            if item
                .session_ids
                .clarifier
                .as_deref()
                .is_some_and(|s| !s.is_empty())
            {
                return false;
            }
            true
        })
        .collect();

    let mut jobs: Vec<usize> = Vec::new();
    for idx in clarifying {
        if dry_run {
            dry_actions.push(format!(
                "would re-clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
        } else {
            jobs.push(idx);
        }
    }

    if jobs.is_empty() {
        return;
    }

    // Spawn all re-clarifications as detached async tasks.
    // Results are polled by tick_clarify_poll on subsequent ticks.
    let hint = "(review the context above and decide if the item is ready)";
    for &idx in &jobs {
        let session_id = global_infra::uuid::Uuid::v4().to_string();
        let item = &mut items[idx];
        item.session_ids.clarifier = Some(session_id.clone());
        item.last_activity_at = Some(global_types::now_rfc3339());

        if let Err(e) = crate::io::queries::tasks::persist_reclarify_start(pool, item).await {
            tracing::error!(
                module = "captain",
                id = item.id,
                error = %e,
                "failed to persist reclarify start"
            );
            item.session_ids.clarifier = None;
            continue;
        }

        let cwd = match clarifier::resolve_clarifier_cwd(item, config) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "cannot resolve cwd for async reclarifier"
                );
                let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("cannot resolve reclarifier cwd: {e}"),
                );
                continue;
            }
        };

        if let Err(e) = crate::io::headless_cc::log_cc_session(
            pool,
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &session_id,
                cwd: &cwd,
                model: &workflow.models.clarifier,
                caller: "clarifier",
                cost_usd: None,
                duration_ms: None,
                resumed: item.session_ids.clarifier.is_some(),
                task_id: Some(item.id),
                status: global_types::SessionStatus::Running,
                worker_name: "",
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        {
            tracing::warn!(
                module = "captain",
                id = item.id,
                error = %e,
                "failed to log reclarifier session start"
            );
        }

        let task = item.clone();
        let workflow = workflow.clone();
        let config = config.clone();
        let pool = pool.clone();
        let hint = hint.to_string();
        let session_id_for_panic = session_id.clone();
        let cwd_for_failure = cwd.clone();
        let task_id_num = task.id;

        task_tracker.spawn(async move {
            let result = AssertUnwindSafe(async {
                match clarifier::answer_and_reclarify(&task, &hint, &workflow, &config, &pool).await
                {
                    Ok(_result) => {
                        tracing::info!(
                            module = "captain",
                            %session_id,
                            "async reclarifier completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            module = "captain",
                            %session_id,
                            error = %e,
                            "async reclarifier failed"
                        );
                        let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                        global_claude::write_error_result(
                            &stream_path,
                            &format!("reclarifier failed: {e}"),
                        );
                        let error_text = format!("{e}");
                        let api_error_status = e
                            .downcast_ref::<global_claude::CcError>()
                            .and_then(|cc| cc.api_error_status());
                        if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                            &pool,
                            &session_id,
                            &cwd_for_failure,
                            "clarifier",
                            Some(task_id_num),
                            Some(&error_text),
                            api_error_status,
                        )
                        .await
                        {
                            tracing::warn!(
                                module = "captain",
                                %session_id,
                                error = %e2,
                                "log_cc_failure failed"
                            );
                        }
                    }
                }
            })
            .catch_unwind()
            .await;

            if let Err(panic) = result {
                tracing::error!(
                    module = "captain",
                    session_id = %session_id_for_panic,
                    "async reclarifier panicked: {:?}",
                    panic
                );
                let stream_path =
                    global_infra::paths::stream_path_for_session(&session_id_for_panic);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("reclarifier panicked: {:?}", panic),
                );
            }
        });
    }
}
