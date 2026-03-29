-- Rename session status "done" → "stopped" (SessionStatus enum formalization).
UPDATE cc_sessions SET status = 'stopped' WHERE status = 'done';
