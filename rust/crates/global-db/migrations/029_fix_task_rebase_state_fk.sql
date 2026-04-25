-- Fix task_rebase_state FK reference.
--
-- Migration 011 renamed `tasks_new` → `tasks`, but SQLite stored the FK
-- string `REFERENCES tasks_new(id)` literally in task_rebase_state's schema
-- and did not rewrite it on rename. With PRAGMA foreign_keys = ON, every
-- DELETE against task_rebase_state then fails with
-- `no such table: main.tasks_new`, which in turn fails every captain
-- tick that clears rebase state for a task — including the first tick on
-- any fresh sandbox database.
--
-- Fix: rebuild the table with the correct FK target (tasks), preserving
-- existing rows. Uses the standard SQLite table-rebuild recipe under
-- legacy_alter_table=OFF so further renames would update FK refs
-- properly, but the immediate rebuild does not rely on that.

CREATE TABLE task_rebase_state_fixed (
    task_id    INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    worker     TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    retries    INTEGER NOT NULL DEFAULT 0,
    head_sha   TEXT
);

INSERT INTO task_rebase_state_fixed (task_id, worker, status, retries, head_sha)
SELECT task_id, worker, status, retries, head_sha FROM task_rebase_state;

DROP TABLE task_rebase_state;
ALTER TABLE task_rebase_state_fixed RENAME TO task_rebase_state;
