-- Make tasks.workbench_id a NOT NULL FK to workbenches(id).
--
-- Migration 005 added `workbench_id INTEGER` (nullable, no FK). Migration
-- 023 backfilled NULLs to 0 but left both the nullability and the missing
-- FK in place — the "NOT NULL enforced at app level" comment in 023 is
-- aspirational. Pre-spawn tasks have lived under a `workbench_id = 0`
-- sentinel ever since.
--
-- This migration eliminates that gap. The lifecycle is also moving so
-- that workbenches are created at task birth (eager creation) rather
-- than at spawn time, which means every task can carry a real workbench
-- id from its first INSERT. Any row that pre-dates that change and is
-- still pointing at workbench `0` (or NULL, or a dangling id) is hard
-- deleted before the rebuild.
--
-- ON DELETE CASCADE on `tasks.workbench_id` is for *DB referential
-- integrity*, not a disk-cleanup hook. Application paths (delete a task,
-- archive a workbench, or the background `run_workbench_cleanup` GC)
-- always go through `io::task_cleanup::cleanup_task` first, which
-- removes the on-disk worktree and the `mando/<slug>` branch and then
-- soft-deletes the workbench row (`mark_deleted`). No production code
-- hard-deletes a workbench. If a future code path needs to hard-delete,
-- it must invoke `cleanup_task` first; otherwise the cascade will
-- correctly drop the task row but leak the on-disk worktree.
--
-- Rebuild recipe follows migration 011 / 029. PRAGMA foreign_keys is
-- toggled off by the pool wrapper for the duration of this script (it
-- recognizes the OFF directive below) and restored to ON at the end.

PRAGMA foreign_keys = OFF;

-- 1. Hard-delete any task that cannot satisfy the new FK.
DELETE FROM tasks
WHERE workbench_id IS NULL
   OR workbench_id = 0
   OR workbench_id NOT IN (SELECT id FROM workbenches);

-- 2. Rebuild tasks with NOT NULL FK and ON DELETE CASCADE.
CREATE TABLE tasks_new (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    title                   TEXT    NOT NULL,
    status                  TEXT    NOT NULL DEFAULT 'new',
    project_id              INTEGER NOT NULL REFERENCES projects(id),
    workbench_id            INTEGER NOT NULL REFERENCES workbenches(id) ON DELETE CASCADE,
    worker                  TEXT,
    resource                TEXT,
    context                 TEXT,
    original_prompt         TEXT,
    created_at              TEXT,
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
    rev                     INTEGER NOT NULL DEFAULT 1,
    reopened_at             TEXT,
    planning                INTEGER NOT NULL DEFAULT 0,
    no_auto_merge           INTEGER NOT NULL DEFAULT 0,
    paused_until            INTEGER,
    is_bug_fix              INTEGER NOT NULL DEFAULT 0
);

INSERT INTO tasks_new (
    id, title, status, project_id, workbench_id, worker, resource, context,
    original_prompt, created_at, pr_number, worker_started_at, intervention_count,
    captain_review_trigger, session_ids, last_activity_at, plan, no_pr, reopen_seq,
    reopen_source, images, review_fail_count, clarifier_fail_count, spawn_fail_count,
    merge_fail_count, escalation_report, source, worker_seq, rev, reopened_at,
    planning, no_auto_merge, paused_until, is_bug_fix
)
SELECT
    id, title, status, project_id, workbench_id, worker, resource, context,
    original_prompt, created_at, pr_number, worker_started_at, intervention_count,
    captain_review_trigger, session_ids, last_activity_at, plan, no_pr, reopen_seq,
    reopen_source, images, review_fail_count, clarifier_fail_count, spawn_fail_count,
    merge_fail_count, escalation_report, source, worker_seq, rev, reopened_at,
    planning, no_auto_merge, paused_until, is_bug_fix
FROM tasks;

-- 3. Rebuild task_rebase_state so its FK string points at the new
-- tasks table (migration 029 documents the SQLite-stores-FK-as-text
-- gotcha — same recipe applies on every tasks rename).
CREATE TABLE task_rebase_state_new (
    task_id    INTEGER PRIMARY KEY REFERENCES tasks_new(id) ON DELETE CASCADE,
    worker     TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    retries    INTEGER NOT NULL DEFAULT 0,
    head_sha   TEXT
);

INSERT INTO task_rebase_state_new (task_id, worker, status, retries, head_sha)
SELECT task_id, worker, status, retries, head_sha
FROM task_rebase_state
WHERE task_id IN (SELECT id FROM tasks_new);

DROP TABLE task_rebase_state;
ALTER TABLE task_rebase_state_new RENAME TO task_rebase_state;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- 4. Recreate indexes (matching the post-011 set).
CREATE INDEX IF NOT EXISTS idx_tasks_status     ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_worker     ON tasks(worker);
CREATE INDEX IF NOT EXISTS idx_tasks_source     ON tasks(source);
CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);

-- The pool migration runner restores `PRAGMA foreign_keys = ON` after
-- this script completes (see `global-db/src/pool.rs::install_migrations`),
-- so no closing PRAGMA is needed here.
