ALTER TABLE tasks ADD COLUMN review_fail_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN clarifier_fail_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN spawn_fail_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN merge_fail_count INTEGER NOT NULL DEFAULT 0;

UPDATE tasks
SET review_fail_count = retry_count
WHERE status = 'captain-reviewing';

UPDATE tasks
SET clarifier_fail_count = retry_count
WHERE status IN ('new', 'clarifying');

UPDATE tasks
SET spawn_fail_count = retry_count
WHERE status IN ('queued', 'rework');

UPDATE tasks
SET merge_fail_count = retry_count
WHERE status = 'captain-merging';
