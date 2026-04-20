-- Remove duplicate terminal/verdict timeline events before table rebuild.
-- These event types should occur at most once per task per lifecycle position,
-- so duplicates are always erroneous. Repeatable events (worker_nudged,
-- captain_review_started, worker_spawned) are left untouched.
-- Keep the first occurrence (lowest id) for each (task_id, event_type, summary) group.
DELETE FROM timeline_events
WHERE event_type IN (
    'merged', 'awaiting_review', 'captain_review_verdict',
    'escalated', 'completed_no_pr', 'canceled'
)
AND id NOT IN (
    SELECT MIN(id)
    FROM timeline_events
    WHERE event_type IN (
        'merged', 'awaiting_review', 'captain_review_verdict',
        'escalated', 'completed_no_pr', 'canceled'
    )
    GROUP BY task_id, event_type, summary
);

-- Rebuild the table with dedupe_key as NOT NULL.
-- SQLite cannot ALTER COLUMN to add NOT NULL, so we recreate.
CREATE TABLE timeline_events_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL,
    actor      TEXT    NOT NULL DEFAULT 'captain',
    summary    TEXT    NOT NULL DEFAULT '',
    data       TEXT    NOT NULL DEFAULT '{}',
    dedupe_key TEXT    NOT NULL
);

-- Copy existing rows with backfilled dedupe keys.
INSERT INTO timeline_events_new (id, task_id, event_type, timestamp, actor, summary, data, dedupe_key)
SELECT id, task_id, event_type, timestamp, actor, summary, data,
       task_id || '-' || event_type || '-legacy-' || id
FROM timeline_events;

DROP TABLE timeline_events;
ALTER TABLE timeline_events_new RENAME TO timeline_events;

-- Recreate indexes on the new table.
CREATE INDEX idx_timeline_task_ts      ON timeline_events(task_id, timestamp);
CREATE INDEX idx_timeline_type         ON timeline_events(event_type);
CREATE INDEX idx_timeline_task_type_ts ON timeline_events(task_id, event_type, timestamp DESC);
CREATE UNIQUE INDEX idx_timeline_dedupe ON timeline_events(dedupe_key);
