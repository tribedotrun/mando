-- Extend persisted session provider values for OpenCode/GLM worker sessions.
PRAGMA foreign_keys = OFF;

CREATE TABLE cc_sessions_new (
    session_id        TEXT PRIMARY KEY,
    created_at        TEXT NOT NULL,
    caller            TEXT NOT NULL,
    cwd               TEXT NOT NULL DEFAULT '',
    model             TEXT NOT NULL DEFAULT '',
    status            TEXT NOT NULL DEFAULT 'stopped',
    cost_usd          REAL,
    duration_ms       INTEGER,
    resumed           INTEGER NOT NULL DEFAULT 0,
    turn_count        INTEGER NOT NULL DEFAULT 1,
    task_id           INTEGER,
    scout_item_id     INTEGER,
    worker_name       TEXT,
    resumed_at        TEXT,
    credential_id     INTEGER,
    rev               INTEGER NOT NULL DEFAULT 1,
    error             TEXT,
    api_error_status  INTEGER,
    result_applied_at TEXT,
    provider          TEXT NOT NULL DEFAULT 'claude'
        CHECK (provider IN ('claude', 'codex', 'opencode'))
);

INSERT INTO cc_sessions_new (
    session_id, created_at, caller, cwd, model, status,
    cost_usd, duration_ms, resumed, turn_count,
    task_id, scout_item_id, worker_name, resumed_at, credential_id,
    rev, error, api_error_status, result_applied_at, provider
)
SELECT
    session_id, created_at, caller, cwd, model, status,
    cost_usd, duration_ms, resumed, turn_count,
    task_id, scout_item_id, worker_name, resumed_at, credential_id,
    rev, error, api_error_status, result_applied_at, provider
FROM cc_sessions;

DROP TABLE cc_sessions;
ALTER TABLE cc_sessions_new RENAME TO cc_sessions;

CREATE INDEX IF NOT EXISTS idx_cc_sessions_caller ON cc_sessions(caller);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_status ON cc_sessions(status);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_ts ON cc_sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_task_id ON cc_sessions(task_id);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_scout ON cc_sessions(scout_item_id);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_credential ON cc_sessions(credential_id);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_provider ON cc_sessions(provider);

PRAGMA foreign_keys = ON;
