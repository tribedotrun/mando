//! Test-only helpers for seeding FK-required rows.

/// Insert a workbench row and return its id.
///
/// Every test that inserts a `Task` must call this first — migration 033
/// made `tasks.workbench_id` a NOT NULL FK referencing `workbenches(id)`.
pub(crate) async fn seed_workbench(pool: &sqlx::SqlitePool, project_id: i64) -> i64 {
    let now = global_types::now_rfc3339();
    let unique = format!("/tmp/mando-test-wb-{}", global_infra::uuid::Uuid::v4());
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO workbenches (project_id, worktree, title, created_at, last_activity_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(project_id)
    .bind(&unique)
    .bind("test-workbench")
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await
    .expect("seed workbench")
}
