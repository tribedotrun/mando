-- Add per-task auto-merge opt-out flag.
ALTER TABLE tasks ADD COLUMN no_auto_merge INTEGER NOT NULL DEFAULT 0;
