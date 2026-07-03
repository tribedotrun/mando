-- Track Codex shell picks for load-balancing when cc_sessions has no row yet.
ALTER TABLE credentials ADD COLUMN last_picked_at INTEGER;