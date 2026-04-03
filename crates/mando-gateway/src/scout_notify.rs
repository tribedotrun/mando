//! Notification emission for scout item processing.

use mando_shared::EventBus;

/// Emit a `BusEvent::Notification` with `ScoutProcessed` kind for a processed item.
pub(crate) async fn emit_scout_processed(bus: &EventBus, pool: &sqlx::SqlitePool, id: i64) {
    let item = match mando_scout::get_scout_item(pool, id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(scout_id = id, error = %e, "scout notification: item lookup failed");
            return;
        }
    };

    let title = item["title"].as_str().unwrap_or("Untitled").to_string();
    let relevance = item["relevance"].as_i64().unwrap_or(0);
    let quality = item["quality"].as_i64().unwrap_or(0);
    let source = item["source_name"].as_str().map(|s| s.to_string());
    let telegraph_url = item["telegraphUrl"].as_str().map(|s| s.to_string());

    let esc_title = mando_shared::escape_html(&title);
    let source_label = source
        .as_deref()
        .map(|s| format!(" — {}", mando_shared::escape_html(s)))
        .unwrap_or_default();
    let message = format!(
        "📰 <b>{esc_title}</b>{source_label}\n\
         Relevance {relevance}/100 · Quality {quality}/100"
    );

    let payload = mando_types::events::NotificationPayload {
        message,
        level: mando_types::NotifyLevel::Normal,
        kind: mando_types::events::NotificationKind::ScoutProcessed {
            scout_id: id,
            title,
            relevance,
            quality,
            source_name: source,
            telegraph_url,
        },
        task_key: Some(format!("scout:{id}")),
        reply_markup: None,
    };

    if let Ok(json) = serde_json::to_value(&payload) {
        bus.send(mando_types::BusEvent::Notification, Some(json));
    }
}
