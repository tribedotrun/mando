-- Remove the clarifier_questions column from tasks.
-- Questions are already stored in timeline_events (ClarifyQuestion events)
-- and this column was only used as an ephemeral cache during active clarification.
ALTER TABLE tasks DROP COLUMN clarifier_questions;

-- Compound index for efficient "latest event of type X for task Y" queries.
CREATE INDEX IF NOT EXISTS idx_timeline_task_type_ts
    ON timeline_events(task_id, event_type, timestamp DESC);
