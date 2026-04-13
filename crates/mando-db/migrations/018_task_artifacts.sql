CREATE TABLE IF NOT EXISTS task_artifacts (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id  INTEGER NOT NULL REFERENCES tasks(id),
    artifact_type TEXT NOT NULL,  -- 'evidence' | 'pr_summary'
    content  TEXT    NOT NULL,    -- markdown text
    media    TEXT,                -- JSON array: [{index, filename, ext, local_path, remote_url}]
    created_at TEXT NOT NULL      -- RFC 3339 timestamp
);

CREATE INDEX IF NOT EXISTS idx_task_artifacts_task_time
    ON task_artifacts (task_id, created_at);
