-- Track terminal workbenches that need auto-titling via Haiku.
-- Set when a CC session starts; cleared after successful title generation
-- or expiry. Survives daemon restarts.
ALTER TABLE workbenches ADD COLUMN pending_title_session TEXT;
