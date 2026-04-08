-- Audit cleanup: drop dead tables/columns, add missing index.

-- Drop dead voice tables (no Rust code reads or writes them).
DROP TABLE IF EXISTS voice_messages;
DROP TABLE IF EXISTS voice_tts_usage;

-- Drop dead retry_count column (not in TaskRow, never read or written).
ALTER TABLE tasks DROP COLUMN retry_count;

-- Add index for archived_at (used in WHERE clause of nearly every task query).
CREATE INDEX IF NOT EXISTS idx_tasks_archived ON tasks(archived_at);
