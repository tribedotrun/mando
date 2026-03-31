//! TaskStore — async SQLite-backed task persistence via mando-db.

use std::collections::HashMap;

use anyhow::Result;
use mando_db::queries::{rebase, sessions, tasks};
use mando_types::rebase_state::RebaseState;
use mando_types::session::SessionEntry;
use mando_types::task::{Task, TaskRouting, TaskUpdateError};
use sqlx::SqlitePool;
use tracing::warn;

pub struct TaskStore {
    pool: SqlitePool,
}

impl TaskStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn routing(&self) -> Result<Vec<TaskRouting>> {
        tasks::routing(&self.pool).await
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<Task>> {
        tasks::find_by_id(&self.pool, id).await
    }

    pub async fn find_by_linear_id(&self, linear_id: &str) -> Result<Option<Task>> {
        tasks::find_by_linear_id(&self.pool, linear_id).await
    }

    pub async fn load_all(&self) -> Result<Vec<Task>> {
        let mut tasks = tasks::load_all(&self.pool).await?;
        hydrate_rebase_state(&self.pool, &mut tasks).await;
        Ok(tasks)
    }

    pub async fn load_all_with_archived(&self) -> Result<Vec<Task>> {
        let mut tasks = tasks::load_all_with_archived(&self.pool).await?;
        hydrate_rebase_state(&self.pool, &mut tasks).await;
        Ok(tasks)
    }

    pub async fn add(&self, task: Task) -> Result<i64> {
        tasks::insert_task(&self.pool, &task).await
    }

    pub async fn write_task(&self, task: &Task) -> Result<bool> {
        tasks::update_task(&self.pool, task).await
    }

    pub async fn remove(&self, id: i64) -> Result<bool> {
        tasks::remove(&self.pool, id).await
    }

    pub async fn update(&self, id: i64, f: impl FnOnce(&mut Task)) -> Result<bool> {
        let mut task = self
            .find_by_id(id)
            .await?
            .ok_or(TaskUpdateError::NotFound(id))?;
        f(&mut task);
        tasks::update_task(&self.pool, &task).await
    }

    pub async fn update_fields(&self, id: i64, updates: &serde_json::Value) -> Result<()> {
        let mut task = self
            .find_by_id(id)
            .await?
            .ok_or(TaskUpdateError::NotFound(id))?;
        if updates.get("status").and_then(|v| v.as_str()).is_some() && task.status.is_finalized() {
            return Err(TaskUpdateError::TerminalStatusTransition(
                task.status.as_str().to_string(),
            )
            .into());
        }
        apply_json_updates(&mut task, updates)?;
        tasks::update_task(&self.pool, &task).await?;
        Ok(())
    }

    pub(crate) async fn force_update_fields(
        &self,
        id: i64,
        updates: &serde_json::Value,
    ) -> Result<()> {
        let mut task = self
            .find_by_id(id)
            .await?
            .ok_or(TaskUpdateError::NotFound(id))?;
        apply_json_updates(&mut task, updates)?;
        tasks::update_task(&self.pool, &task).await?;
        Ok(())
    }

    pub(crate) async fn archive_terminal(&self, grace_secs: u64) -> Result<usize> {
        tasks::archive_terminal(&self.pool, grace_secs).await
    }

    /// Merge tick-changed items into the store, preserving concurrent human edits.
    ///
    /// For items with a pre-tick snapshot, uses 3-way merge (base vs tick-changed vs current DB).
    /// For items without a snapshot (new items), upserts directly.
    /// All writes are wrapped in a single transaction for atomicity.
    /// Also persists rebase state changes to the `task_rebase_state` table.
    pub(crate) async fn merge_changed_items(
        &self,
        pre_tick_snapshot: &HashMap<i64, serde_json::Value>,
        changed_items: &[Task],
    ) -> Result<()> {
        tasks::merge_changed_items(
            &self.pool,
            pre_tick_snapshot,
            changed_items,
            merge_task_changes,
        )
        .await?;

        // Persist rebase state for any task that has rebase fields set.
        // Delete stale rebase state for tasks where all fields are cleared.
        for task in changed_items {
            if task.rebase_worker.is_some()
                || task.rebase_retries > 0
                || task.rebase_head_sha.is_some()
            {
                let state = RebaseState {
                    task_id: task.id,
                    worker: task.rebase_worker.clone(),
                    status: mando_types::rebase_state::RebaseStatus::default(),
                    retries: task.rebase_retries,
                    head_sha: task.rebase_head_sha.clone(),
                };
                if let Err(e) = rebase::upsert(&self.pool, &state).await {
                    warn!(task_id = task.id, error = %e, "failed to persist rebase state");
                }
            } else if task.id > 0 {
                if let Err(e) = rebase::delete(&self.pool, task.id).await {
                    warn!(task_id = task.id, error = %e, "failed to delete cleared rebase state");
                }
            }
        }

        Ok(())
    }

    pub async fn replace_all(&self, tasks_list: Vec<Task>) -> Result<()> {
        tasks::replace_all(&self.pool, &tasks_list).await
    }

    pub(crate) async fn status_counts(&self) -> Result<HashMap<String, usize>> {
        tasks::status_counts(&self.pool).await
    }

    pub async fn has_active_with_source(&self, source: &str) -> Result<bool> {
        tasks::has_active_with_source(&self.pool, source).await
    }

    pub async fn active_worker_count(&self) -> Result<usize> {
        tasks::active_worker_count(&self.pool).await
    }

    // -- Session methods --

    pub async fn upsert_session(&self, entry: &SessionEntry) -> Result<()> {
        sessions::upsert_session(
            &self.pool,
            &sessions::SessionUpsert {
                session_id: &entry.session_id,
                created_at: &entry.ts,
                caller: &entry.caller,
                cwd: &entry.cwd,
                model: &entry.model,
                status: entry
                    .status
                    .parse()
                    .unwrap_or(mando_types::SessionStatus::Stopped),
                cost_usd: entry.cost_usd,
                duration_ms: entry.duration_ms,
                resumed: entry.resumed,
                task_id: if entry.task_id.is_empty() {
                    None
                } else {
                    Some(entry.task_id.as_str())
                },
                scout_item_id: None,
                worker_name: if entry.worker_name.is_empty() {
                    None
                } else {
                    Some(entry.worker_name.as_str())
                },
            },
        )
        .await
    }

    pub async fn list_sessions(
        &self,
        page: usize,
        per_page: usize,
        group: Option<&str>,
    ) -> Result<(Vec<mando_db::queries::sessions::SessionRow>, usize)> {
        sessions::list_sessions(&self.pool, page, per_page, group).await
    }

    pub async fn list_sessions_for_task(
        &self,
        task_id: &str,
    ) -> Vec<mando_db::queries::sessions::SessionRow> {
        sessions::list_sessions_for_task(&self.pool, task_id)
            .await
            .unwrap_or_default()
    }

    pub async fn session_cwd(&self, session_id: &str) -> Option<String> {
        sessions::session_cwd(&self.pool, session_id)
            .await
            .ok()
            .flatten()
    }

    pub async fn total_session_cost(&self) -> f64 {
        sessions::total_session_cost(&self.pool)
            .await
            .unwrap_or(0.0)
    }

    pub async fn category_counts(&self) -> HashMap<String, usize> {
        sessions::category_counts(&self.pool)
            .await
            .unwrap_or_default()
    }
}

/// Hydrate rebase fields on tasks from the `task_rebase_state` table.
///
/// Loads all rebase state rows in a single query, then matches by task_id.
async fn hydrate_rebase_state(pool: &SqlitePool, tasks: &mut [Task]) {
    let states = match rebase::all(pool).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(module = "task_store", error = %e, "failed to load rebase state — tasks will lack rebase fields");
            return;
        }
    };
    if states.is_empty() {
        return;
    }
    let map: HashMap<i64, _> = states.into_iter().map(|s| (s.task_id, s)).collect();
    for task in tasks.iter_mut() {
        if let Some(state) = map.get(&task.id) {
            task.rebase_worker = state.worker.clone();
            task.rebase_retries = state.retries;
            task.rebase_head_sha = state.head_sha.clone();
        }
    }
}

pub(crate) fn task_snapshot(task: &Task) -> Result<serde_json::Value> {
    serde_json::to_value(task).map_err(|e| anyhow::anyhow!("task serialization failed: {e}"))
}

fn merge_task_changes(
    base_snapshot: &serde_json::Value,
    changed: &Task,
    current: &Task,
) -> Result<Task> {
    let base_obj = base_snapshot
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("task snapshot must be a JSON object"))?;
    let changed_snapshot = task_snapshot(changed)?;
    let changed_obj = changed_snapshot
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("changed task snapshot must be a JSON object"))?;

    let mut merged_obj = task_snapshot(current)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("current task snapshot must be a JSON object"))?;

    for (key, changed_value) in changed_obj {
        if base_obj.get(key) != Some(changed_value) {
            merged_obj.insert(key.clone(), changed_value.clone());
        }
    }

    serde_json::from_value(serde_json::Value::Object(merged_obj))
        .map_err(|e| anyhow::anyhow!("failed to deserialize merged task: {e}"))
}

fn apply_json_updates(task: &mut Task, updates: &serde_json::Value) -> Result<()> {
    let obj = updates.as_object().ok_or(TaskUpdateError::NotAnObject)?;
    for (k, v) in obj {
        if v.is_null() {
            task.clear_field(k)?;
        } else {
            task.set_field(k, v)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mando_types::task::ItemStatus;

    async fn test_store() -> TaskStore {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        TaskStore::new(db.pool().clone())
    }

    #[tokio::test]
    async fn open_empty() {
        let store = test_store().await;
        assert!(store.load_all().await.unwrap().is_empty());
        assert!(store.routing().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_and_find() {
        let store = test_store().await;
        let mut task = Task::new("Test task");
        task.status = ItemStatus::New;
        task.project = Some("org/repo".into());
        let id = store.add(task).await.unwrap();
        assert!(id > 0);

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.title, "Test task");
        assert_eq!(found.project.as_deref(), Some("org/repo"));
    }

    #[tokio::test]
    async fn update_task() {
        let store = test_store().await;
        let task = Task::new("Update me");
        let id = store.add(task).await.unwrap();

        store
            .update(id, |t| t.status = ItemStatus::Queued)
            .await
            .unwrap();
        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.status, ItemStatus::Queued);
    }

    #[tokio::test]
    async fn remove_task() {
        let store = test_store().await;
        let id = store.add(Task::new("Remove me")).await.unwrap();
        assert!(store.remove(id).await.unwrap());
        assert!(store.find_by_id(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn status_counts() {
        let store = test_store().await;
        let mut t1 = Task::new("A");
        t1.status = ItemStatus::New;
        store.add(t1).await.unwrap();
        let mut t2 = Task::new("B");
        t2.status = ItemStatus::Queued;
        store.add(t2).await.unwrap();

        let counts = store.status_counts().await.unwrap();
        assert_eq!(counts.get("new"), Some(&1));
        assert_eq!(counts.get("queued"), Some(&1));
    }

    #[tokio::test]
    async fn update_fields_rejects_invalid_status() {
        let store = test_store().await;
        let id = store.add(Task::new("Strict task")).await.unwrap();

        let err = store
            .update_fields(id, &serde_json::json!({"status": "not-a-real-status"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid status"));

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.status, ItemStatus::New);
    }

    #[tokio::test]
    async fn update_fields_rejects_invalid_numeric_field() {
        let store = test_store().await;
        let id = store.add(Task::new("Strict number")).await.unwrap();

        let err = store
            .update_fields(id, &serde_json::json!({"intervention_count": "oops"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid field type"));

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.intervention_count, 0);
    }

    #[tokio::test]
    async fn merge_changed_items_preserves_concurrent_human_edits() {
        let store = test_store().await;
        let id = store.add(Task::new("Concurrent edit")).await.unwrap();
        let original = store.find_by_id(id).await.unwrap().unwrap();
        let snapshots = HashMap::from([(id, task_snapshot(&original).unwrap())]);

        let mut tick_copy = original.clone();
        tick_copy.status = ItemStatus::Queued;

        store
            .update_fields(id, &serde_json::json!({"context": "human-edit"}))
            .await
            .unwrap();
        store
            .merge_changed_items(&snapshots, &[tick_copy])
            .await
            .unwrap();

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.status, ItemStatus::Queued);
        assert_eq!(found.context.as_deref(), Some("human-edit"));
    }
}
