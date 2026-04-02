//! Cron job executor — creates tasks when cron jobs fire.

use std::sync::Arc;

use mando_shared::cron::service::JobCallback;
use mando_types::events::{NotificationKind, NotificationPayload};
use mando_types::notify::NotifyLevel;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::info;

use arc_swap::ArcSwap;

/// Build the cron job callback that creates tasks via the standard pipeline.
///
/// When a cron job fires:
/// 1. Check `features.cron` — skip if disabled
/// 2. Check for an active (non-finalized) task with source `cron:{job_id}`
/// 3. If one exists, skip (dedup)
/// 4. Otherwise, create a new task via `dashboard::add_task`
/// 5. Set `source` on the new task
/// 6. Publish a Tasks bus event
pub fn make_cron_callback(
    config: Arc<ArcSwap<mando_config::Config>>,
    task_store: Arc<RwLock<mando_captain::io::task_store::TaskStore>>,
    bus: Arc<mando_shared::EventBus>,
) -> JobCallback {
    Arc::new(move |job: mando_types::CronJob| {
        let config = config.clone();
        let task_store = task_store.clone();
        let bus = bus.clone();
        Box::pin(async move {
            // Gate: respect the cron feature flag.
            if !config.load().features.cron {
                info!(
                    module = "cron-executor",
                    job_id = %job.id,
                    "cron feature disabled, skipping"
                );
                return Ok(());
            }

            let source = format!("cron:{}", job.id);
            let title = if job.payload.message.is_empty() {
                job.name.clone()
            } else {
                job.payload.message.clone()
            };
            let project = job.cwd.as_deref();
            let config_snapshot = config.load_full();

            // sqlx pool handles concurrency internally.
            let store = task_store.read().await;

            // Dedup: skip if there's already an active task for this cron job.
            // Fails closed — if the DB query errors, we refuse to create.
            let has_active = store
                .has_active_with_source(&source)
                .await
                .map_err(|e| format!("dedup check failed: {e}"))?;
            if has_active {
                info!(
                    module = "cron-executor",
                    job_id = %job.id,
                    job_name = %job.name,
                    source = %source,
                    "dedup: active task exists, skipping"
                );
                return Ok(());
            }

            // Create task via the same pipeline as user-created todos.
            let result = mando_captain::runtime::dashboard::add_task(
                &config_snapshot,
                &store,
                &title,
                project,
            )
            .await
            .map_err(|e| format!("failed to create task: {e}"))?;

            // Set the source tag on the newly created task.
            let task_id = result["id"]
                .as_i64()
                .ok_or_else(|| "add_task did not return an id".to_string())?;

            store
                .update_fields(task_id, &json!({"source": &source}))
                .await
                .map_err(|e| format!("failed to set source on task {task_id}: {e}"))?;

            // Drop the read lock before logging and bus notification.
            drop(store);

            info!(
                module = "cron-executor",
                job_id = %job.id,
                job_name = %job.name,
                task_id = task_id,
                source = %source,
                "created task from cron job"
            );

            // Notify Electron / SSE subscribers.
            bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "add", "source": "cron", "cron_job_id": job.id})),
            );

            // Telegram / notification subscribers.
            let safe_name = job
                .name
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let payload = NotificationPayload {
                message: format!("⏰ Cron <b>{safe_name}</b> — created task #{task_id}"),
                level: NotifyLevel::Normal,
                kind: NotificationKind::CronAlert {
                    action_id: job.id.clone(),
                },
                task_key: Some(format!("cron:{}", job.id)),
                reply_markup: None,
            };
            match serde_json::to_value(&payload) {
                Ok(val) => bus.send(mando_types::BusEvent::Notification, Some(val)),
                Err(e) => {
                    tracing::warn!(module = "cron-executor", error = %e, "failed to serialize notification payload")
                }
            }

            Ok(())
        })
    })
}
