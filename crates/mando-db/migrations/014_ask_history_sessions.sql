-- Add ask_id and session_id columns to ask_history for multi-session Q&A.
-- ask_id groups messages into conversations; session_id tracks the CC session.

PRAGMA foreign_keys = OFF;

CREATE TABLE ask_history_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id),
    ask_id     TEXT    NOT NULL,
    session_id TEXT    NOT NULL,
    role       TEXT    NOT NULL,
    content    TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL
);

INSERT INTO ask_history_new (id, task_id, ask_id, session_id, role, content, timestamp)
    SELECT id, task_id, 'legacy', 'legacy', role, content, timestamp
    FROM ask_history;

DROP TABLE ask_history;
ALTER TABLE ask_history_new RENAME TO ask_history;

CREATE INDEX idx_ask_history_task ON ask_history(task_id, timestamp);
CREATE INDEX idx_ask_history_ask  ON ask_history(ask_id, timestamp);

PRAGMA foreign_keys = ON;
