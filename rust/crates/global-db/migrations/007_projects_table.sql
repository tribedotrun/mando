-- Add projects table, convert tasks.project + workbenches.project to FK,
-- convert pr to integer, drop github_repo from tasks.

-- ── Projects table ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS projects (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    UNIQUE NOT NULL,
    path        TEXT    NOT NULL DEFAULT '',
    github_repo TEXT
);

-- Seed from existing task data (includes github_repo from the old column).
INSERT OR IGNORE INTO projects (name, github_repo)
SELECT project, MAX(github_repo)
FROM tasks WHERE project IS NOT NULL
GROUP BY project;

-- Also seed from workbenches (name only, no github_repo available).
INSERT OR IGNORE INTO projects (name)
SELECT DISTINCT project FROM workbenches WHERE project != '';

-- ── Rebuild tasks with project_id FK + pr_number integer ────────────────────

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
    archived_at             TEXT,
    worker_seq              INTEGER NOT NULL DEFAULT 0
);

INSERT INTO tasks_new (
    id, title, status, project_id, worker, resource, context, original_prompt,
    created_at, workbench_id, pr_number, worker_started_at, intervention_count,
    captain_review_trigger, session_ids, last_activity_at, plan, no_pr,
    reopen_seq, reopen_source, images, review_fail_count, clarifier_fail_count,
    spawn_fail_count, merge_fail_count, escalation_report, source, archived_at,
    worker_seq
)
SELECT
    t.id, t.title, t.status,
    COALESCE(p.id, (SELECT MIN(id) FROM projects)),
    t.worker, t.resource, t.context, t.original_prompt,
    t.created_at, t.workbench_id,
    CASE WHEN t.pr IS NOT NULL AND t.pr != '' THEN CAST(t.pr AS INTEGER) ELSE NULL END,
    t.worker_started_at, t.intervention_count,
    t.captain_review_trigger, t.session_ids, t.last_activity_at, t.plan, t.no_pr,
    t.reopen_seq, t.reopen_source, t.images, t.review_fail_count, t.clarifier_fail_count,
    t.spawn_fail_count, t.merge_fail_count, t.escalation_report, t.source, t.archived_at,
    t.worker_seq
FROM tasks t
LEFT JOIN projects p ON p.name = t.project;

-- Migrate task_rebase_state (FK CASCADE to tasks).
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

-- ── Migrate workbenches.project TEXT -> project_id INTEGER ──────────────────

ALTER TABLE workbenches ADD COLUMN project_id INTEGER NOT NULL DEFAULT 0;
UPDATE workbenches SET project_id = COALESCE(
    (SELECT p.id FROM projects p WHERE p.name = workbenches.project),
    (SELECT MIN(id) FROM projects)
);

-- Rebuild workbenches to drop the old project TEXT column.
CREATE TABLE workbenches_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id  INTEGER NOT NULL REFERENCES projects(id),
    worktree    TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL,
    created_at  TEXT    NOT NULL,
    archived_at TEXT,
    deleted_at  TEXT
);
INSERT INTO workbenches_new (id, project_id, worktree, title, created_at, archived_at, deleted_at)
SELECT id, project_id, worktree, title, created_at, archived_at, deleted_at FROM workbenches;
DROP TABLE workbenches;
ALTER TABLE workbenches_new RENAME TO workbenches;
CREATE INDEX idx_workbenches_project_id ON workbenches (project_id);

-- ── Recreate task indexes ───────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_tasks_status     ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_worker     ON tasks(worker);
CREATE INDEX IF NOT EXISTS idx_tasks_source     ON tasks(source);
CREATE INDEX IF NOT EXISTS idx_tasks_archived   ON tasks(archived_at);
CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);
