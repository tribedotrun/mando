-- Workbench: owns a git worktree under a project.
-- Tasks belong to workbenches via workbench_id FK.

CREATE TABLE workbenches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project     TEXT    NOT NULL,
    worktree    TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL,
    created_at  TEXT    NOT NULL,
    archived_at TEXT,
    deleted_at  TEXT
);

CREATE INDEX idx_workbenches_project ON workbenches (project);

-- Add workbench_id to tasks (nullable — pre-dispatch tasks have no workbench).
ALTER TABLE tasks ADD COLUMN workbench_id INTEGER;

-- Migrate existing tasks that have a worktree into workbench rows.
-- Use OR IGNORE in case two tasks share the same worktree path.
INSERT OR IGNORE INTO workbenches (project, worktree, title, created_at)
SELECT
    COALESCE(project, ''),
    worktree,
    title,
    COALESCE(created_at, datetime('now'))
FROM tasks
WHERE worktree IS NOT NULL AND worktree != '';

-- Link migrated tasks to their new workbench rows.
UPDATE tasks
SET workbench_id = (
    SELECT w.id FROM workbenches w WHERE w.worktree = tasks.worktree
)
WHERE worktree IS NOT NULL AND worktree != '';

-- Drop the worktree column from tasks (now lives on workbenches).
ALTER TABLE tasks DROP COLUMN worktree;
