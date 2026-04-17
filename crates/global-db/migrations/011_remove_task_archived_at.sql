-- Remove archived_at from tasks. Archive is solely a workbench concern.
-- Migrate any task-level archived_at to the parent workbench first.
-- Disable FK enforcement during table rebuild (timeline_events, ask_history
-- reference tasks; DROP TABLE would fail with FK enabled).
PRAGMA foreign_keys = OFF;

UPDATE workbenches
SET archived_at = (
    SELECT t.archived_at FROM tasks t
    WHERE t.workbench_id = workbenches.id
      AND t.archived_at IS NOT NULL
    LIMIT 1
)
WHERE archived_at IS NULL
  AND id IN (
    SELECT t.workbench_id FROM tasks t
    WHERE t.workbench_id IS NOT NULL AND t.archived_at IS NOT NULL
  );

-- Cancel orphan archived tasks (archived but no workbench -- legacy data).
-- These cannot be archived via workbench, so mark them canceled to prevent
-- them from resurfacing as active after the column is dropped.
UPDATE tasks SET status = 'canceled'
WHERE archived_at IS NOT NULL AND workbench_id IS NULL AND status NOT IN ('canceled');

-- Rebuild tasks without archived_at column.

CREATE TABLE tasks_new (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    title                   TEXT    NOT NULL,
    status                  TEXT    NOT NULL DEFAULT 'new',
    project_id              INTEGER NOT NULL REFERENCES projects(id),
    worker                  TEXT,
    resource                TEXT,
    context                 TEXT,
    original_prompt         TEXT,
    created_at              TEXT,
    workbench_id            INTEGER,
    pr_number               INTEGER,
    worker_started_at       TEXT,
    intervention_count      INTEGER NOT NULL DEFAULT 0,
    captain_review_trigger  TEXT,
    session_ids             TEXT    NOT NULL DEFAULT '{}',
    last_activity_at        TEXT,
    plan                    TEXT,
    no_pr                   INTEGER NOT NULL DEFAULT 0,
    reopen_seq              INTEGER NOT NULL DEFAULT 0,
    reopen_source           TEXT,
    images                  TEXT,
    review_fail_count       INTEGER NOT NULL DEFAULT 0,
    clarifier_fail_count    INTEGER NOT NULL DEFAULT 0,
    spawn_fail_count        INTEGER NOT NULL DEFAULT 0,
    merge_fail_count        INTEGER NOT NULL DEFAULT 0,
    escalation_report       TEXT,
    source                  TEXT,
    worker_seq              INTEGER NOT NULL DEFAULT 0,
    rev                     INTEGER NOT NULL DEFAULT 1
);

INSERT INTO tasks_new (
    id, title, status, project_id, worker, resource, context, original_prompt,
    created_at, workbench_id, pr_number, worker_started_at, intervention_count,
    captain_review_trigger, session_ids, last_activity_at, plan, no_pr,
    reopen_seq, reopen_source, images, review_fail_count, clarifier_fail_count,
    spawn_fail_count, merge_fail_count, escalation_report, source, worker_seq, rev
)
SELECT
    id, title, status, project_id, worker, resource, context, original_prompt,
    created_at, workbench_id, pr_number, worker_started_at, intervention_count,
    captain_review_trigger, session_ids, last_activity_at, plan, no_pr,
    reopen_seq, reopen_source, images, review_fail_count, clarifier_fail_count,
    spawn_fail_count, merge_fail_count, escalation_report, source, worker_seq, rev
FROM tasks;

-- Migrate task_rebase_state FK.
CREATE TABLE task_rebase_state_new (
    task_id    INTEGER PRIMARY KEY REFERENCES tasks_new(id) ON DELETE CASCADE,
    worker     TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    retries    INTEGER NOT NULL DEFAULT 0,
    head_sha   TEXT
);
INSERT INTO task_rebase_state_new SELECT * FROM task_rebase_state;
DROP TABLE task_rebase_state;
ALTER TABLE task_rebase_state_new RENAME TO task_rebase_state;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- Recreate indexes (without the old archived index).
CREATE INDEX IF NOT EXISTS idx_tasks_status     ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_worker     ON tasks(worker);
CREATE INDEX IF NOT EXISTS idx_tasks_source     ON tasks(source);
CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);

-- Re-enable FK enforcement.
PRAGMA foreign_keys = ON;
