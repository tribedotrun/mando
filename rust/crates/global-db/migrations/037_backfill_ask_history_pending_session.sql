-- Backfill `ask_history.session_id` rows that were stored as the `'pending'`
-- placeholder. Migration 014 created the column and stamped pre-migration
-- rows as `'legacy'`. Code that persists a question (or an error) before the
-- CC session id is known stamps the row as `'pending'` and previously never
-- backfilled, so the question text was permanently dissociated from its CC
-- session.
--
-- For each `(task_id, ask_id)` group there is at most one assistant row
-- carrying the real CC session id. Copy that id onto every `'pending'` row
-- in the same group. Rows whose group has no recoverable assistant id (the
-- CC call failed before producing a session id) stay `'pending'`. `'legacy'`
-- rows are not touched.

UPDATE ask_history
SET session_id = (
    SELECT a.session_id
    FROM ask_history a
    WHERE a.task_id = ask_history.task_id
      AND a.ask_id = ask_history.ask_id
      AND a.role = 'assistant'
      AND a.session_id NOT IN ('pending', 'legacy')
    LIMIT 1
)
WHERE session_id = 'pending'
  AND EXISTS (
    SELECT 1
    FROM ask_history a
    WHERE a.task_id = ask_history.task_id
      AND a.ask_id = ask_history.ask_id
      AND a.role = 'assistant'
      AND a.session_id NOT IN ('pending', 'legacy')
  );
