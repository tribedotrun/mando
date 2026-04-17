-- Backfill any legacy NULL workbench_id values to 0.
-- The NOT NULL constraint is enforced at the application level (Rust type
-- is i64, not Option<i64>).  SQLite cannot ALTER COLUMN to add NOT NULL
-- without a full table rebuild, which is fragile across migration versions.
UPDATE tasks SET workbench_id = 0 WHERE workbench_id IS NULL;
