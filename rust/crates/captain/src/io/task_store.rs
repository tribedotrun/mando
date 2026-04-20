//! TaskStore — async SQLite-backed task persistence via mando-db.

use std::collections::HashMap;

use crate::io::queries::{rebase, tasks};
use crate::{RebaseState, Task, TaskRouting, TaskUpdateError, UpdateTaskInput};
use anyhow::{Context, Result};
use sessions_db as session_queries;
use sqlx::SqlitePool;

/// Opaque JSON snapshot of a [`Task`] used for the 3-way merge algorithm.
///
/// Wraps a `serde_json::Value` so the merge layer can detect Option-field
/// deletions via key absence (`skip_serializing_if = "Option::is_none"` omits
/// a `None` field entirely, which is the merge signal). The newtype prevents
/// raw `Value` from escaping into the rest of the codebase.
#[derive(PartialEq)]
pub(crate) struct TaskSnapshotJson(serde_json::Value);

impl TaskSnapshotJson {
    /// Serialize a task into its snapshot form.
    pub(crate) fn from_task(task: &Task) -> Result<Self> {
        let v = serde_json::to_value(task)
            .map_err(|e| anyhow::anyhow!("task serialization failed: {e}"))?;
        Ok(Self(v))
    }

    /// Return the inner JSON object map, if the snapshot is an object (it always is).
    fn as_object(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.0.as_object()
    }
}

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

    #[must_use = "find_by_id returns a Future that must be awaited"]
    pub async fn find_by_id(&self, id: i64) -> Result<Option<Task>> {
        tasks::find_by_id(&self.pool, id).await
    }

    pub async fn load_all(&self) -> Result<Vec<Task>> {
        let mut tasks = tasks::load_all(&self.pool).await?;
        hydrate_rebase_state(&self.pool, &mut tasks).await?;
        Ok(tasks)
    }

    pub async fn load_all_with_archived(&self) -> Result<Vec<Task>> {
        let mut tasks = tasks::load_all_with_archived(&self.pool).await?;
        hydrate_rebase_state(&self.pool, &mut tasks).await?;
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

    pub async fn update_fields(&self, id: i64, updates: UpdateTaskInput) -> Result<()> {
        let mut task = self
            .find_by_id(id)
            .await?
            .ok_or(TaskUpdateError::NotFound(id))?;
        task.apply_update(updates);
        tasks::update_task(&self.pool, &task).await?;
        Ok(())
    }

    pub(crate) async fn set_planning(&self, id: i64, planning: bool) -> Result<()> {
        let mut task = self
            .find_by_id(id)
            .await?
            .ok_or(TaskUpdateError::NotFound(id))?;
        task.planning = planning;
        tasks::update_task(&self.pool, &task).await?;
        Ok(())
    }

    pub(crate) async fn archive_terminal_workbenches(
        &self,
        grace: std::time::Duration,
    ) -> Result<usize> {
        crate::io::queries::workbenches::archive_terminal(&self.pool, grace.as_secs()).await
    }

    /// Merge tick-changed items into the store, preserving concurrent human edits.
    ///
    /// For items with a pre-tick snapshot, uses 3-way merge (base vs tick-changed vs current DB).
    /// For items without a snapshot (new items), upserts directly.
    /// All writes are wrapped in a single transaction for atomicity.
    /// Also persists rebase state changes to the `task_rebase_state` table.
    /// Any rebase state error fails the whole merge so the tick reports it.
    pub(crate) async fn merge_changed_items(
        &self,
        pre_tick_snapshot: &HashMap<i64, TaskSnapshotJson>,
        changed_items: &[Task],
    ) -> Result<()> {
        tasks::merge_changed_items(
            &self.pool,
            pre_tick_snapshot,
            changed_items,
            merge_task_changes,
        )
        .await
        .context("merge_changed_items: task update transaction")?;

        // Persist rebase state for any task that has rebase fields set.
        // Delete stale rebase state for tasks where all fields are cleared.
        // Errors here propagate; the tick must not silently drop rebase state.
        for task in changed_items {
            if task.rebase_worker.is_some()
                || task.rebase_retries > 0
                || task.rebase_head_sha.is_some()
            {
                let state = RebaseState {
                    task_id: task.id,
                    worker: task.rebase_worker.clone(),
                    status: crate::RebaseStatus::default(),
                    retries: task.rebase_retries,
                    head_sha: task.rebase_head_sha.clone(),
                };
                rebase::upsert(&self.pool, &state)
                    .await
                    .with_context(|| format!("persist rebase state for task {}", task.id))?;
            } else if task.id > 0 {
                rebase::delete(&self.pool, task.id)
                    .await
                    .with_context(|| format!("delete cleared rebase state for task {}", task.id))?;
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

    pub async fn daily_merge_counts(&self, days: u32) -> Result<Vec<(String, i64)>> {
        tasks::daily_merge_counts(&self.pool, days).await
    }

    /// Today's date in the server's local timezone, formatted as YYYY-MM-DD.
    /// Delegates to SQLite's `DATE('now', 'localtime')` so the boundary
    /// matches what recent-merges queries compute.
    pub async fn today_localtime_iso(&self) -> Result<String> {
        tasks::today_localtime_iso(&self.pool).await
    }

    // -- Session methods --

    pub async fn list_sessions(
        &self,
        page: usize,
        per_page: usize,
        group: Option<&str>,
        status: Option<&str>,
    ) -> Result<(Vec<session_queries::SessionRow>, usize)> {
        session_queries::list_sessions(&self.pool, page, per_page, group, status).await
    }

    pub async fn list_sessions_for_task(
        &self,
        task_id: i64,
    ) -> Result<Vec<session_queries::SessionRow>> {
        session_queries::list_sessions_for_task(&self.pool, task_id)
            .await
            .with_context(|| format!("list_sessions_for_task {}", task_id))
    }

    pub async fn session_cwd(&self, session_id: &str) -> Result<Option<String>> {
        session_queries::session_cwd(&self.pool, session_id)
            .await
            .with_context(|| format!("session_cwd {}", session_id))
    }

    pub async fn total_session_cost(&self) -> Result<f64> {
        session_queries::total_session_cost(&self.pool)
            .await
            .context("total_session_cost")
    }

    pub async fn category_counts(&self) -> Result<HashMap<String, usize>> {
        session_queries::category_counts(&self.pool)
            .await
            .context("category_counts")
    }
}

/// Hydrate rebase fields on tasks from the `task_rebase_state` table.
///
/// Loads all rebase state rows in a single query, then matches by task_id.
async fn hydrate_rebase_state(pool: &SqlitePool, tasks: &mut [Task]) -> Result<()> {
    let states = rebase::all(pool)
        .await
        .context("load rebase state for hydration")?;
    if states.is_empty() {
        return Ok(());
    }
    let map: HashMap<i64, _> = states.into_iter().map(|s| (s.task_id, s)).collect();
    for task in tasks.iter_mut() {
        if let Some(state) = map.get(&task.id) {
            task.rebase_worker = state.worker.clone();
            task.rebase_retries = state.retries;
            task.rebase_head_sha = state.head_sha.clone();
        }
    }
    Ok(())
}

fn merge_task_changes(
    base_snapshot: &TaskSnapshotJson,
    changed: &Task,
    current: &Task,
) -> Result<Task> {
    let base_obj = base_snapshot
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("task snapshot must be a JSON object"))?;
    let changed_snapshot = TaskSnapshotJson::from_task(changed)?;
    let changed_obj = changed_snapshot
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("changed task snapshot must be a JSON object"))?;

    let current_snapshot = TaskSnapshotJson::from_task(current)?;
    let current_obj = current_snapshot
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("current task snapshot must be a JSON object"))?;

    let mut merged_obj = current_obj.clone();

    // Apply tick modifications: keys present in changed that differ from base.
    for (key, changed_value) in changed_obj {
        if base_obj.get(key) != Some(changed_value) {
            merged_obj.insert(key.clone(), changed_value.clone());
        }
    }

    // Apply tick deletions: keys present in base but absent in changed were
    // cleared to None by the tick (Option fields with `skip_serializing_if`
    // are omitted when None). Honor the clear unless the human concurrently
    // changed the field to a different value.
    for key in base_obj.keys() {
        if !changed_obj.contains_key(key) {
            // The tick cleared this field. Preserve the clear unless the human
            // concurrently modified it (current differs from base).
            if current_obj.get(key) == base_obj.get(key) {
                merged_obj.remove(key);
            }
        }
    }

    serde_json::from_value(serde_json::Value::Object(merged_obj))
        .map_err(|e| anyhow::anyhow!("failed to deserialize merged task: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ItemStatus;

    async fn test_store() -> TaskStore {
        let db = global_db::Db::open_in_memory().await.unwrap();
        // Seed a test project so FK constraints are satisfied.
        settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        TaskStore::new(db.pool().clone())
    }

    fn test_task(title: &str) -> Task {
        let mut t = Task::new(title);
        t.project_id = 1;
        t.project = "test".into();
        t
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
        let mut task = test_task("Test task");
        task.status = ItemStatus::New;
        let id = store.add(task).await.unwrap();
        assert!(id > 0);

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.title, "Test task");
        assert_eq!(found.project.as_str(), "test");
    }

    #[tokio::test]
    async fn update_task() {
        let store = test_store().await;
        let task = test_task("Update me");
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
        let id = store.add(test_task("Remove me")).await.unwrap();
        assert!(store.remove(id).await.unwrap());
        assert!(store.find_by_id(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn status_counts() {
        let store = test_store().await;
        let mut t1 = test_task("A");
        t1.status = ItemStatus::New;
        store.add(t1).await.unwrap();
        let mut t2 = test_task("B");
        t2.status = ItemStatus::Queued;
        store.add(t2).await.unwrap();

        let counts = store.status_counts().await.unwrap();
        assert_eq!(counts.get("new"), Some(&1));
        assert_eq!(counts.get("queued"), Some(&1));
    }

    #[tokio::test]
    async fn update_fields_sets_context() {
        let store = test_store().await;
        let id = store.add(test_task("Strict task")).await.unwrap();

        store
            .update_fields(
                id,
                UpdateTaskInput {
                    context: Some(Some("ctx".into())),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.context.as_deref(), Some("ctx"));
        assert_eq!(found.status, ItemStatus::New);
    }

    #[tokio::test]
    async fn update_fields_sets_intervention_count() {
        let store = test_store().await;
        let id = store.add(test_task("Counter task")).await.unwrap();

        store
            .update_fields(
                id,
                UpdateTaskInput {
                    intervention_count: Some(5),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.intervention_count, 5);
    }

    #[tokio::test]
    async fn merge_changed_items_preserves_concurrent_human_edits() {
        let store = test_store().await;
        let id = store.add(test_task("Concurrent edit")).await.unwrap();
        let original = store.find_by_id(id).await.unwrap().unwrap();
        let snapshots = HashMap::from([(id, TaskSnapshotJson::from_task(&original).unwrap())]);

        let mut tick_copy = original.clone();
        tick_copy.status = ItemStatus::Queued;

        store
            .update_fields(
                id,
                UpdateTaskInput {
                    context: Some(Some("human-edit".into())),
                    ..Default::default()
                },
            )
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

    /// Regression test: when the tick clears an Option field to None (e.g.
    /// rework clearing `pr_number`), the 3-way merge must persist the deletion
    /// even though `skip_serializing_if` omits the key from the JSON.
    #[tokio::test]
    async fn merge_propagates_field_cleared_to_none() {
        let store = test_store().await;
        let mut task = test_task("Rework merge test");
        task.pr_number = Some(594);
        task.worker = Some("worker-1".into());
        let id = store.add(task).await.unwrap();
        let original = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(original.pr_number, Some(594));
        let snapshots = HashMap::from([(id, TaskSnapshotJson::from_task(&original).unwrap())]);

        // Simulate transition_rework_to_queued clearing fields.
        let mut tick_copy = original.clone();
        tick_copy.status = ItemStatus::Queued;
        tick_copy.pr_number = None;
        tick_copy.worker = None;

        store
            .merge_changed_items(&snapshots, &[tick_copy])
            .await
            .unwrap();

        let found = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.status, ItemStatus::Queued);
        assert!(
            found.pr_number.is_none(),
            "pr_number must be cleared, got {:?}",
            found.pr_number
        );
        assert!(
            found.worker.is_none(),
            "worker must be cleared, got {:?}",
            found.worker
        );
    }

    /// When the tick clears a field but a human concurrently set it to a
    /// new value, the human's value wins (it is more recent).
    #[tokio::test]
    async fn merge_clear_preserves_concurrent_human_set() {
        let store = test_store().await;
        let mut task = test_task("Concurrent set vs clear");
        task.pr_number = Some(594);
        let id = store.add(task).await.unwrap();
        let original = store.find_by_id(id).await.unwrap().unwrap();
        let snapshots = HashMap::from([(id, TaskSnapshotJson::from_task(&original).unwrap())]);

        // Tick clears pr_number.
        let mut tick_copy = original.clone();
        tick_copy.pr_number = None;

        // Human concurrently sets pr_number to a new value.
        store
            .update_fields(
                id,
                UpdateTaskInput {
                    pr_number: Some(Some(607)),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        store
            .merge_changed_items(&snapshots, &[tick_copy])
            .await
            .unwrap();

        let found = store.find_by_id(id).await.unwrap().unwrap();
        // Human's update should win since they changed it concurrently.
        assert_eq!(found.pr_number, Some(607));
    }
}
