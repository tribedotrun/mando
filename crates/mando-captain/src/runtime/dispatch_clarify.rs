//! Parallel clarification dispatch — spawns clarifier sessions as background tasks.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_shared::EventBus;
use mando_types::task::{ItemStatus, Task};
use mando_types::BusEvent;

use crate::biz::dispatch_logic;
use crate::runtime::clarifier;
use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;

struct ClarifyJob {
    idx: usize,
    session_id: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn clarify_new_items(
    items: &mut [Task],
    config: &Config,
    active_workers: usize,
    max_workers: usize,
    workflow: &CaptainWorkflow,
    _notifier: &Notifier,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    _alerts: &mut Vec<String>,
    _resource_limits: &HashMap<String, usize>,
    _max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
    bus: Option<&EventBus>,
) {
    let new_items = dispatch_logic::new_items(items);
    if new_items.is_empty() {
        return;
    }

    // Skip clarification entirely when no worker slots are available.
    if active_workers >= max_workers {
        return;
    }

    // Cap parallel clarifications at available worker slots to avoid
    // overwhelming the LLM provider with a burst of concurrent sessions.
    let max_parallel = max_workers.saturating_sub(active_workers);

    // Phase 1: Pre-process — set Clarifying status, persist to DB, log sessions.
    let mut jobs: Vec<ClarifyJob> = Vec::new();
    for idx in new_items {
        if jobs.len() >= max_parallel {
            break;
        }
        if dry_run {
            dry_actions.push(format!(
                "would clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
            continue;
        }

        let session_id = mando_uuid::Uuid::v4().to_string();
        let item = &mut items[idx];
        item.status = ItemStatus::Clarifying;
        item.last_activity_at = Some(mando_types::now_rfc3339());
        item.session_ids.clarifier = Some(session_id.clone());

        if let Err(e) = mando_db::queries::tasks::persist_clarify_start(pool, item).await {
            tracing::error!(
                module = "captain",
                id = item.id,
                error = %e,
                "failed to persist clarify start — skipping clarifier this tick"
            );
            item.status = ItemStatus::New;
            item.session_ids.clarifier = None;
            continue;
        }

        let clarifier_cwd = match crate::runtime::clarifier::resolve_clarifier_cwd(item, config) {
            Ok(cwd) => cwd,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "cannot log clarifier session start, skipping this dispatch"
                );
                item.session_ids.clarifier = None;
                continue;
            }
        };
        if let Err(e) = crate::io::headless_cc::log_cc_session(
            pool,
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &session_id,
                cwd: &clarifier_cwd,
                model: &workflow.models.clarifier,
                caller: "clarifier",
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: &item.id.to_string(),
                status: mando_types::SessionStatus::Running,
                worker_name: "",
            },
        )
        .await
        {
            tracing::warn!(module = "captain", id = item.id, error = %e, "failed to log clarifier session start");
        }

        let _ = super::timeline_emit::emit_for_task(
            item,
            mando_types::timeline::TimelineEventType::ClarifyStarted,
            "Clarification starting",
            serde_json::json!({"session_id": &session_id}),
            pool,
        )
        .await;

        jobs.push(ClarifyJob { idx, session_id });
    }

    if jobs.is_empty() {
        return;
    }

    emit_live_refresh(bus);

    // Phase 2: Spawn all clarifications as detached async tasks.
    // Each task runs the CC session and writes results to the stream file.
    // The tick continues immediately — results are polled by
    // tick_clarify_poll on subsequent ticks.
    for job in jobs {
        let task = items[job.idx].clone();
        let workflow = workflow.clone();
        let config = config.clone();
        let pool = pool.clone();
        let session_id = job.session_id.clone();

        let session_id_for_panic = session_id.clone();
        let cwd = match clarifier::resolve_clarifier_cwd(&task, &config) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = task.id,
                    error = %e,
                    "cannot resolve cwd for async clarifier — writing error"
                );
                let stream_path = mando_config::stream_path_for_session(&session_id);
                mando_cc::write_error_result(
                    &stream_path,
                    &format!("cannot resolve clarifier cwd: {e}"),
                );
                continue;
            }
        };
        let cwd_for_failure = cwd.clone();
        let task_id = task.id.to_string();

        tokio::spawn(async move {
            let result = AssertUnwindSafe(async {
                match clarifier::run_clarification(
                    &task,
                    &workflow,
                    &config,
                    &pool,
                    Some(&session_id),
                )
                .await
                {
                    Ok(_result) => {
                        // run_clarification already logged the session as Stopped
                        // and the CC process wrote results to the stream file.
                        tracing::info!(
                            module = "captain",
                            %session_id,
                            "async clarifier completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            module = "captain",
                            %session_id,
                            error = %e,
                            "async clarifier failed"
                        );
                        let stream_path = mando_config::stream_path_for_session(&session_id);
                        mando_cc::write_error_result(
                            &stream_path,
                            &format!("clarifier failed: {e}"),
                        );
                        if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                            &pool,
                            &session_id,
                            &cwd_for_failure,
                            "clarifier",
                            &task_id,
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
                    "async clarifier panicked: {:?}",
                    panic
                );
                let stream_path = mando_config::stream_path_for_session(&session_id_for_panic);
                mando_cc::write_error_result(
                    &stream_path,
                    &format!("clarifier panicked: {:?}", panic),
                );
            }
        });
    }
}

fn emit_live_refresh(bus: Option<&EventBus>) {
    if let Some(bus) = bus {
        bus.send(BusEvent::Tasks, None);
        bus.send(BusEvent::Sessions, None);
    }
}
