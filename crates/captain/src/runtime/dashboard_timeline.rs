//! Dashboard timeline endpoint -- GET /api/tasks/{id}/timeline.

use anyhow::Result;

pub async fn get_item_timeline(
    item_id: &str,
    last_n: Option<usize>,
    pool: &sqlx::SqlitePool,
) -> Result<serde_json::Value> {
    let task_id_num: i64 = item_id.parse().unwrap_or(0);

    let events = match last_n {
        Some(n) => crate::io::queries::timeline::load_last_n(pool, task_id_num, n as i64).await?,
        None => crate::io::queries::timeline::load(pool, task_id_num).await?,
    };

    Ok(serde_json::to_value(&events)?)
}
