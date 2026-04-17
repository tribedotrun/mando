-- Scout research runs: audit trail for async research jobs.
CREATE TABLE IF NOT EXISTS scout_research_runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    research_prompt TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'running',
    error           TEXT,
    session_id      TEXT,
    added_count     INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL,
    completed_at    TEXT
);

-- Link scout items back to the research run that discovered them.
ALTER TABLE scout_items ADD COLUMN research_run_id INTEGER REFERENCES scout_research_runs(id);
CREATE INDEX idx_scout_items_research_run ON scout_items(research_run_id);
