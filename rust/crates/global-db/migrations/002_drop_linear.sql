-- Remove Linear integration artifacts.

DROP TABLE IF EXISTS linear_workpad;

DROP INDEX IF EXISTS idx_tasks_linear_id;

ALTER TABLE tasks DROP COLUMN linear_id;
