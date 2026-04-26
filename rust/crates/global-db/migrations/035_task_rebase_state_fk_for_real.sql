-- Repair task_rebase_state.task_id FK after the tasks_new → tasks rename in
-- migration 034.
--
-- Migration 029 already documented the SQLite quirk: when ALTER TABLE RENAME
-- runs with `legacy_alter_table = ON`, SQLite does NOT propagate the rename
-- to FK references stored in dependent tables' schema text. The author of
-- 029 (and 034) assumed `legacy_alter_table = OFF`, but that is the modern
-- default — the SQLite CLI shipped with macOS 14 / 15 still defaults to ON,
-- and the bash bootstrap path that primes test fixtures inherits that
-- default. The Rust pool also runs migrations against connections where the
-- pragma was never explicitly flipped, so the same drift hits dev / prod
-- daemons started from a fresh DB.
--
-- Concretely: migration 034 created `task_rebase_state_new` with
-- `REFERENCES tasks_new(id)` (the temporary rebuild table), then later
-- renamed `tasks_new → tasks`. Under legacy mode that rename did not
-- rewrite the FK literal, so `task_rebase_state.task_id REFERENCES
-- tasks_new(id)` survived. Every captain tick that calls
-- `rebase::delete(task_id)` then trips
--   error code 1: "no such table: main.tasks_new"
-- which surfaces as a 500 on POST /api/captain/tick and bricks every
-- integration test that exercises the captain state machine.
--
-- Recipe (mirrors 029 but with the legacy pragma forced OFF first so the
-- subsequent rename DOES rewrite FKs in dependents — safe even though we
-- don't strictly need that here, since we're recreating
-- `task_rebase_state` directly with `REFERENCES tasks(id)`):

PRAGMA legacy_alter_table = OFF;

CREATE TABLE task_rebase_state_repaired (
    task_id    INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    worker     TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    retries    INTEGER NOT NULL DEFAULT 0,
    head_sha   TEXT
);

INSERT INTO task_rebase_state_repaired (task_id, worker, status, retries, head_sha)
SELECT task_id, worker, status, retries, head_sha FROM task_rebase_state;

DROP TABLE task_rebase_state;
ALTER TABLE task_rebase_state_repaired RENAME TO task_rebase_state;
