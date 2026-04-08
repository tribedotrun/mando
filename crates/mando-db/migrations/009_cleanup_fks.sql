-- Drop ghost tables, fix cc_sessions.task_id type, add missing FKs.
-- Temporarily disable FK enforcement for table rebuilds.
PRAGMA foreign_keys = OFF;

-- ── Drop ghost tables (cron, learning, analytics -- removed in code) ────────

DROP TABLE IF EXISTS cron_jobs;
DROP TABLE IF EXISTS task_decisions;
DROP TABLE IF EXISTS task_patterns;

-- ── Rebuild cc_sessions with task_id INTEGER + FK constraints ───────────────

CREATE TABLE cc_sessions_new (
    session_id      TEXT    PRIMARY KEY,
    created_at      TEXT    NOT NULL,
    caller          TEXT    NOT NULL,
    cwd             TEXT    NOT NULL DEFAULT '',
    model           TEXT    NOT NULL DEFAULT '',
    status          TEXT    NOT NULL DEFAULT 'stopped',
    cost_usd        REAL,
    duration_ms     INTEGER,
    resumed         INTEGER NOT NULL DEFAULT 0,
    turn_count      INTEGER NOT NULL DEFAULT 1,
    task_id         INTEGER,
    scout_item_id   INTEGER,
    worker_name     TEXT
);

INSERT INTO cc_sessions_new (
    session_id, created_at, caller, cwd, model, status,
    cost_usd, duration_ms, resumed, turn_count,
    task_id, scout_item_id, worker_name
)
SELECT
    session_id, created_at, caller, cwd, model, status,
    cost_usd, duration_ms, resumed, turn_count,
    CASE WHEN task_id IS NOT NULL AND task_id != '' THEN CAST(task_id AS INTEGER) ELSE NULL END,
    scout_item_id, worker_name
FROM cc_sessions;

DROP TABLE cc_sessions;
ALTER TABLE cc_sessions_new RENAME TO cc_sessions;

CREATE INDEX idx_cc_sessions_caller    ON cc_sessions(caller);
CREATE INDEX idx_cc_sessions_status    ON cc_sessions(status);
CREATE INDEX idx_cc_sessions_ts        ON cc_sessions(created_at);
CREATE INDEX idx_cc_sessions_task_id   ON cc_sessions(task_id);
CREATE INDEX idx_cc_sessions_scout     ON cc_sessions(scout_item_id);

-- ── Add FK constraints on timeline_events and ask_history ───────────────────
-- These tables already have task_id INTEGER NOT NULL but no REFERENCES clause.
-- Rebuild to add the FK.

CREATE TABLE timeline_events_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id),
    event_type TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL,
    actor      TEXT    NOT NULL DEFAULT 'captain',
    summary    TEXT    NOT NULL DEFAULT '',
    data       TEXT    NOT NULL DEFAULT '{}',
    dedupe_key TEXT    NOT NULL
);

INSERT INTO timeline_events_new SELECT * FROM timeline_events;
DROP TABLE timeline_events;
ALTER TABLE timeline_events_new RENAME TO timeline_events;

CREATE INDEX idx_timeline_task_ts       ON timeline_events(task_id, timestamp);
CREATE INDEX idx_timeline_type          ON timeline_events(event_type);
CREATE INDEX idx_timeline_task_type_ts  ON timeline_events(task_id, event_type, timestamp DESC);
CREATE UNIQUE INDEX idx_timeline_dedupe ON timeline_events(dedupe_key);

CREATE TABLE ask_history_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id),
    role       TEXT    NOT NULL,
    content    TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL
);

INSERT INTO ask_history_new SELECT * FROM ask_history;
DROP TABLE ask_history;
ALTER TABLE ask_history_new RENAME TO ask_history;

CREATE INDEX idx_ask_history_task ON ask_history(task_id, timestamp);

PRAGMA foreign_keys = ON;
