-- Make dedupe_key nullable and drop its UNIQUE constraint.
--
-- persist_status_transition used a state-fingerprint dedupe_key that could
-- legitimately recur across review cycles, causing UNIQUE violations that
-- permanently blocked valid transitions. The UPDATE WHERE status=expected
-- guard is the real idempotency mechanism; the fingerprint was unnecessary.
--
-- The triage system has been refactored to use atomic transactions instead
-- of dedupe-key-based crash recovery, and the backfill system has been
-- retired (Created events are now emitted at task creation time).
--
-- No code writes or reads dedupe_key anymore, but we keep the column
-- (nullable, no index) to avoid breaking existing rows.

PRAGMA foreign_keys = OFF;

CREATE TABLE timeline_events_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id),
    event_type TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL,
    actor      TEXT    NOT NULL DEFAULT 'captain',
    summary    TEXT    NOT NULL DEFAULT '',
    data       TEXT    NOT NULL DEFAULT '{}',
    dedupe_key TEXT
);

INSERT INTO timeline_events_new
    (id, task_id, event_type, timestamp, actor, summary, data, dedupe_key)
SELECT id, task_id, event_type, timestamp, actor, summary, data, dedupe_key
FROM timeline_events;

DROP TABLE timeline_events;
ALTER TABLE timeline_events_new RENAME TO timeline_events;

-- Recreate query indexes (no UNIQUE constraint on dedupe_key).
CREATE INDEX idx_timeline_task_ts      ON timeline_events(task_id, timestamp);
CREATE INDEX idx_timeline_type         ON timeline_events(event_type);
CREATE INDEX idx_timeline_task_type_ts ON timeline_events(task_id, event_type, timestamp DESC);

-- Delete backfill sentinel rows (empty timestamp, source: "backfill").
-- All 71 existing tasks have been backfilled; new tasks emit Created at
-- creation time, so the backfill system is retired.
DELETE FROM timeline_events
WHERE timestamp = '' AND data LIKE '%"source":"backfill"%';

PRAGMA foreign_keys = ON;
