-- Tag tasks the clarifier identified as bug fixes. Threaded through worker
-- and captain-review prompts so the worker reproduces + captures before-state
-- evidence first, and captain enforces both before+after evidence on review.
ALTER TABLE tasks ADD COLUMN is_bug_fix INTEGER NOT NULL DEFAULT 0;
