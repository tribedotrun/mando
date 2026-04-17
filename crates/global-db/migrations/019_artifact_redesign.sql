-- Rename pr_summary -> work_summary
UPDATE task_artifacts SET artifact_type = 'work_summary'
  WHERE artifact_type = 'pr_summary';

-- Add reopened_at to tasks (for freshness checks)
ALTER TABLE tasks ADD COLUMN reopened_at TEXT;
