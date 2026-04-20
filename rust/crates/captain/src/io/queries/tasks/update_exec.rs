use anyhow::Result;
use serde_json::json;

use crate::{Task, TimelineEvent, TimelineEventPayload};

use super::{bind_task_write_fields, infer_transition_command, update_task_sql};

fn inferred_status_event(current: &Task, next: &Task, command: &str) -> TimelineEvent {
    TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "system".to_string(),
        summary: format!(
            "Status changed: {} → {}",
            current.status.as_str(),
            next.status.as_str()
        ),
        data: TimelineEventPayload::StatusChangedByCommand {
            from: current.status.as_str().to_string(),
            to: next.status.as_str().to_string(),
            command: command.to_string(),
        },
    }
}

pub(super) async fn update_task_exec(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    current: &Task,
    task: &Task,
) -> Result<(bool, Option<i64>)> {
    if current.status != task.status {
        let command = infer_transition_command(current.status, task.status, task.planning)?;
        let event = inferred_status_event(current, task, command);
        let bus_payload = super::super::tasks_persist::task_bus_effect(task.id, "updated");
        let touch_payload = super::super::tasks_persist::workbench_touch_effect(task.workbench_id);
        let event_type_str = event.data.event_type_str();
        let metadata = json!({
            "task_id": task.id,
            "event_type": event_type_str,
            "summary": event.summary,
            "actor": event.actor,
        });
        let data_value = super::super::timeline::data_without_tag(&event.data)?;
        let event_payload = json!({
            "task_id": task.id,
            "event_type": event_type_str,
            "timestamp": event.timestamp,
            "actor": event.actor,
            "summary": event.summary,
            "data": data_value,
        });
        let transition_id = super::super::tasks_persist::persist_task_transition_in_tx(
            tx,
            task,
            current.status.as_str(),
            command,
            "system",
            None,
            &metadata,
            &[
                global_db::lifecycle::LifecycleEffect {
                    effect_kind: "task.timeline.project",
                    payload: &event_payload,
                },
                global_db::lifecycle::LifecycleEffect {
                    effect_kind: "task.bus.publish",
                    payload: &bus_payload,
                },
                global_db::lifecycle::LifecycleEffect {
                    effect_kind: "task.workbench.touch",
                    payload: &touch_payload,
                },
            ],
        )
        .await?;
        return Ok((transition_id.is_some(), transition_id));
    }

    let result = bind_task_write_fields(
        sqlx::query(&format!("{} AND rev = ?", update_task_sql())),
        task,
    )
    .bind(task.id)
    .bind(current.rev)
    .execute(&mut **tx)
    .await?;
    Ok((result.rows_affected() > 0, None))
}
