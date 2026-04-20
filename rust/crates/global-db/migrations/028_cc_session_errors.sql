-- Persist CC-session error context on stopped rows so failures are queryable
-- from `mando sessions stream <sid>` without re-reading the stream file.
ALTER TABLE cc_sessions ADD COLUMN error TEXT;
ALTER TABLE cc_sessions ADD COLUMN api_error_status INTEGER;
