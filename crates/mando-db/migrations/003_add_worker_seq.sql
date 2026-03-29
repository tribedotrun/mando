-- Add worker_seq column to tasks for per-task worker sequence numbering.
ALTER TABLE tasks ADD COLUMN worker_seq INTEGER NOT NULL DEFAULT 0;
