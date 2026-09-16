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

    /// Low-level task INSERT that bypasses workbench creation. The DB
    /// FK rejects any task whose `workbench_id` doesn't reference a
    /// real workbench row; production task creation goes through
    /// [`crate::create_task_with_workbench`], which inserts the
    /// workbench and the task in a single transaction. This entry
    /// point supports the captain tick-merge path, which writes
    /// tasks already loaded from the DB with valid FKs. The runtime check
    /// makes a misuse loud and immediate instead of waiting for the
    /// downstream INSERT to fail with a generic FK error.
    pub async fn add(&self, task: Task) -> Result<i64> {
        anyhow::ensure!(
            task.workbench_id > 0,
            "TaskStore::add requires a real workbench_id; \
             new tasks must use create_task_with_workbench"
        );
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

/// Task fields whose value is itself an object of independently-owned slots,
/// merged one level deeper so a tick that touched one slot cannot revert a
/// concurrent write to a sibling slot.
///
/// `session_ids` is the only such field: the tick writes the pre-allocated id
/// into (say) `review`, while `session_retarget` concurrently re-points
/// `merge` at the id CC actually adopted. A whole-object merge treats
/// `session_ids` as one key, sees the tick changed it, and overwrites the
/// retarget — leaving the poller watching a dead session id.
const FIELDWISE_MERGE_KEYS: &[&str] = &["session_ids"];

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

    let merged_obj = merge_json_objects(base_obj, changed_obj, current_obj);

    serde_json::from_value(serde_json::Value::Object(merged_obj))
        .map_err(|e| anyhow::anyhow!("failed to deserialize merged task: {e}"))
}

/// 3-way merge of one JSON object: start from `current` (the DB row, which
/// carries any concurrent write), apply the keys the tick actually changed,
/// then apply the keys the tick cleared.
fn merge_json_objects(
    base_obj: &serde_json::Map<String, serde_json::Value>,
    changed_obj: &serde_json::Map<String, serde_json::Value>,
    current_obj: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut merged_obj = current_obj.clone();

    // Apply tick modifications: keys present in changed that differ from base.
    for (key, changed_value) in changed_obj {
        if base_obj.get(key) == Some(changed_value) {
            continue;
        }
        if FIELDWISE_MERGE_KEYS.contains(&key.as_str()) {
            if let Some(nested) =
                merge_nested_object(base_obj.get(key), changed_value, current_obj.get(key))
            {
                merged_obj.insert(key.clone(), serde_json::Value::Object(nested));
                continue;
            }
        }
        merged_obj.insert(key.clone(), changed_value.clone());
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

    merged_obj
}

/// Recurse one level for a [`FIELDWISE_MERGE_KEYS`] field. Returns `None`
/// when any of the three sides is not an object, so the caller falls back to
/// replacing the whole value.
fn merge_nested_object(
    base: Option<&serde_json::Value>,
    changed: &serde_json::Value,
    current: Option<&serde_json::Value>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let empty = serde_json::Map::new();
    let base_nested = match base {
        Some(v) => v.as_object()?,
        None => &empty,
    };
    let current_nested = match current {
        Some(v) => v.as_object()?,
        None => &empty,
    };
    let changed_nested = changed.as_object()?;
    Some(merge_json_objects(
        base_nested,
        changed_nested,
        current_nested,
    ))
}
