-- Constrain Scout lifecycle status columns to the states accepted by the Rust
-- state machines. Normalize legacy invalid values before rebuilding the tables.
PRAGMA foreign_keys = OFF;

UPDATE scout_research_runs
SET status = 'failed'
WHERE status NOT IN ('running', 'done', 'failed');

CREATE TABLE scout_research_runs_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    research_prompt TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'done', 'failed')),
    error           TEXT,
    session_id      TEXT,
    added_count     INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL,
    completed_at    TEXT,
    rev             INTEGER NOT NULL DEFAULT 1
);

INSERT INTO scout_research_runs_new (
    id,
    research_prompt,
    status,
    error,
    session_id,
    added_count,
    created_at,
    completed_at,
    rev
)
SELECT
    id,
    research_prompt,
    status,
    error,
    session_id,
    added_count,
    created_at,
    completed_at,
    rev
FROM scout_research_runs;

DROP TABLE scout_research_runs;
ALTER TABLE scout_research_runs_new RENAME TO scout_research_runs;

UPDATE scout_items
SET status = 'pending'
WHERE status IS NULL
   OR status NOT IN ('pending', 'fetched', 'processed', 'saved', 'archived', 'error');

CREATE TABLE scout_items_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url             TEXT    UNIQUE NOT NULL,
    type            TEXT    NOT NULL,
    title           TEXT,
    status          TEXT    NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'fetched', 'processed', 'saved', 'archived', 'error')),
    relevance       INTEGER,
    quality         INTEGER,
    date_added      TEXT    NOT NULL,
    date_processed  TEXT,
    added_by        TEXT,
    error_count     INTEGER DEFAULT 0,
    source_name     TEXT,
    date_published  TEXT,
    rev             INTEGER NOT NULL DEFAULT 1,
    summary         TEXT,
    article         TEXT,
    research_run_id INTEGER REFERENCES scout_research_runs(id)
);

INSERT INTO scout_items_new (
    id,
    url,
    type,
    title,
    status,
    relevance,
    quality,
    date_added,
    date_processed,
    added_by,
    error_count,
    source_name,
    date_published,
    rev,
    summary,
    article,
    research_run_id
)
SELECT
    id,
    url,
    type,
    title,
    status,
    relevance,
    quality,
    date_added,
    date_processed,
    added_by,
    error_count,
    source_name,
    date_published,
    rev,
    summary,
    article,
    research_run_id
FROM scout_items;

DROP TABLE scout_items;
ALTER TABLE scout_items_new RENAME TO scout_items;
CREATE INDEX IF NOT EXISTS idx_scout_items_research_run ON scout_items(research_run_id);

PRAGMA foreign_keys = ON;
