//! Typed intents that route handlers emit. Replaces `Vec<(&'static str, Value)>`
//! at the runtime boundary so `enqueue_task_effects` and
//! `persist_task_transition_with_effects` accept named variants with typed payloads.

use api_types::TimelineEventPayload;

/// A single typed effect that captain should run after a task transition.
pub enum EffectRequest {
    /// Publish a task-bus event (e.g. "created" / "updated").
    TaskBusPublish { task_id: i64, action: &'static str },
    /// Touch a workbench to update its last-activity timestamp.
    WorkbenchTouch { workbench_id: i64 },
    /// Project a timeline event row into the outbox.
    TimelineProject {
        task_id: i64,
        timestamp: String,
        actor: &'static str,
        summary: String,
        data: Box<TimelineEventPayload>,
    },
    /// Send a normal-priority Telegram notification.
    NotifyNormal { message: String },
    /// Wake up the captain scheduler.
    WakeupCaptain { reason: &'static str },
}

impl EffectRequest {
    /// The string key expected by the lifecycle outbox processor.
    pub fn into_effect_kind(&self) -> &'static str {
        match self {
            EffectRequest::TaskBusPublish { .. } => "task.bus.publish",
            EffectRequest::WorkbenchTouch { .. } => "task.workbench.touch",
            EffectRequest::TimelineProject { .. } => "task.timeline.project",
            EffectRequest::NotifyNormal { .. } => "task.notify.normal",
            EffectRequest::WakeupCaptain { .. } => "task.wakeup.captain",
        }
    }

    /// Last-mile serialization to the `serde_json::Value` that the
    /// `LifecycleEffect` io boundary expects. All JSON construction
    /// is confined to this single method.
    pub fn into_payload(&self) -> serde_json::Value {
        match self {
            EffectRequest::TaskBusPublish { task_id, action } => {
                serde_json::json!({ "task_id": task_id, "action": action })
            }
            EffectRequest::WorkbenchTouch { workbench_id } => {
                serde_json::json!({ "workbench_id": workbench_id })
            }
            EffectRequest::TimelineProject {
                task_id,
                timestamp,
                actor,
                summary,
                data,
            } => {
                let data_value = data.data_without_tag().unwrap_or(serde_json::Value::Null);
                serde_json::json!({
                    "task_id": task_id,
                    "event_type": data.event_type_str(),
                    "timestamp": timestamp,
                    "actor": actor,
                    "summary": summary,
                    "data": data_value,
                })
            }
            EffectRequest::NotifyNormal { message } => {
                serde_json::json!({ "message": message })
            }
            EffectRequest::WakeupCaptain { reason } => {
                serde_json::json!({ "reason": reason })
            }
        }
    }
}
