-- Add planning mode flag to tasks.
ALTER TABLE tasks ADD COLUMN planning INTEGER NOT NULL DEFAULT 0;
