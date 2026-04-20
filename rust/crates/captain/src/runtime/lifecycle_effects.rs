use std::sync::Arc;

use anyhow::Context;
use global_db::lifecycle::{
    mark_outbox_failed, mark_outbox_processed, pending_outbox_rows, LifecycleOutboxRow,
};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::io::task_store::TaskStore;

#[tracing::instrument(skip_all)]
pub async fn drain_pending(
    pool: &SqlitePool,
    bus: Option<&global_bus::EventBus>,
    task_store: &Arc<RwLock<TaskStore>>,
) -> anyhow::Result<()> {
    let rows = pending_outbox_rows(pool).await?;
    for row in rows {
        if let Err(err) = dispatch_effect(pool, bus, task_store, row.clone()).await {
            mark_outbox_failed(pool, row.id, &err.to_string()).await?;
            return Err(err).context("dispatch lifecycle outbox effect");
        }
        mark_outbox_processed(pool, row.id).await?;
    }
    Ok(())
}

async fn dispatch_effect(
    pool: &SqlitePool,
    bus: Option<&global_bus::EventBus>,
    task_store: &Arc<RwLock<TaskStore>>,
    row: LifecycleOutboxRow,
) -> anyhow::Result<()> {
    let payload: serde_json::Value = serde_json::from_str(&row.payload)
        .with_context(|| format!("decode lifecycle payload {}", row.id))?;
    match row.effect_kind.as_str() {
        "lifecycle.transition.recorded" => Ok(()),
        "task.timeline.project" => {
            let task_id = payload["task_id"]
                .as_i64()
                .context("task.timeline.project missing task_id")?;
            let event_type = payload["event_type"]
                .as_str()
                .context("task.timeline.project missing event_type")?;
            let timestamp = payload["timestamp"]
                .as_str()
                .context("task.timeline.project missing timestamp")?;
            let actor = payload["actor"]
                .as_str()
                .context("task.timeline.project missing actor")?;
            let summary = payload["summary"]
                .as_str()
                .context("task.timeline.project missing summary")?;
            let data = payload
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let data_str = serde_json::to_string(&data)?;
            crate::io::queries::timeline::insert_or_ignore(
                pool,
                task_id,
                event_type,
                timestamp,
                actor,
                summary,
                &data_str,
                &format!("lifecycle-outbox:{}", row.id),
            )
            .await?;
            Ok(())
        }
        "task.bus.publish" => {
            let Some(bus) = bus else {
                return Ok(());
            };
            let task_id = payload["task_id"]
                .as_i64()
                .context("task.bus.publish missing task_id")?;
            let action = payload["action"].as_str().unwrap_or("updated");
            let item_val = task_store
                .read()
                .await
                .find_by_id(task_id)
                .await?
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or(serde_json::Value::Null);
            let item: Option<api_types::TaskItem> = serde_json::from_value(item_val).ok();
            bus.send(global_bus::BusPayload::Tasks(Some(
                api_types::TaskEventData {
                    action: Some(action.to_string()),
                    item,
                    id: Some(task_id),
                    cleared_by: None,
                },
            )));
            Ok(())
        }
        "task.workbench.touch" => {
            let workbench_id = payload["workbench_id"]
                .as_i64()
                .context("task.workbench.touch missing workbench_id")?;
            touch_workbench_activity(pool, bus, workbench_id).await
        }
        "task.notify.normal" => {
            let Some(bus) = bus else {
                return Ok(());
            };
            let message = payload["message"]
                .as_str()
                .context("task.notify.normal missing message")?;
            crate::runtime::notify::Notifier::new(Arc::new(bus.clone()))
                .normal(message)
                .await;
            Ok(())
        }
        "task.wakeup.captain" => {
            crate::WORKER_EXIT_SIGNAL.notify_one();
            Ok(())
        }
        other => anyhow::bail!("unsupported lifecycle effect kind {other}"),
    }
}

async fn touch_workbench_activity(
    pool: &SqlitePool,
    bus: Option<&global_bus::EventBus>,
    workbench_id: i64,
) -> anyhow::Result<()> {
    if workbench_id == 0 {
        return Ok(());
    }
    let touched = crate::io::queries::workbenches::touch_activity(pool, workbench_id).await?;
    if touched {
        if let Some(bus) = bus {
            if let Some(updated) =
                crate::io::queries::workbenches::find_by_id(pool, workbench_id).await?
            {
                let item: Option<api_types::WorkbenchItem> =
                    serde_json::from_value(serde_json::to_value(&updated).unwrap_or_default()).ok();
                bus.send(global_bus::BusPayload::Workbenches(Some(
                    api_types::WorkbenchEventData {
                        action: Some("updated".into()),
                        item,
                    },
                )));
            }
        }
    }
    Ok(())
}
