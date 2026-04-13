ALTER TABLE workbenches ADD COLUMN last_activity_at TEXT;
UPDATE workbenches SET last_activity_at = created_at WHERE last_activity_at IS NULL;
