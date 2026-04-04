//! Dashboard timeline endpoint — GET /api/tasks/{id}/timeline.

use anyhow::Result;

use crate::runtime::timeline_backfill;

pub async fn get_item_timeline(
    item_id: &str,
    last_n: Option<usize>,
    item: Option<&mando_types::Task>,
    pool: &sqlx::SqlitePool,
) -> Result<serde_json::Value> {
    let task_id_num: i64 = item_id.parse().unwrap_or(0);

    if let Some(item) = item {
        timeline_backfill::backfill_if_needed(item, pool).await;
    }

    let mut events = match last_n {
        Some(n) => mando_db::queries::timeline::load_last_n(pool, task_id_num, n as i64).await?,
        None => mando_db::queries::timeline::load(pool, task_id_num).await?,
    };

    // Filter out the backfill marker (empty timestamp + source: "backfill").
    events.retain(|e| {
        !(e.timestamp.is_empty()
            && e.data.get("source").and_then(|v| v.as_str()) == Some("backfill"))
    });

    Ok(serde_json::to_value(&events)?)
}
