-- Persist the coding agent provider selected for a task and each session.
-- Existing rows were created before provider selection and are Claude Code.

ALTER TABLE tasks
    ADD COLUMN provider TEXT NOT NULL DEFAULT 'claude'
    CHECK (provider IN ('claude', 'codex'));

CREATE INDEX IF NOT EXISTS idx_tasks_provider ON tasks(provider);

ALTER TABLE cc_sessions
    ADD COLUMN provider TEXT NOT NULL DEFAULT 'claude'
    CHECK (provider IN ('claude', 'codex'));

CREATE INDEX IF NOT EXISTS idx_cc_sessions_provider ON cc_sessions(provider);
