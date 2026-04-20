--- Add resumed_at timestamp to cc_sessions for tracking last resume time.
ALTER TABLE cc_sessions ADD COLUMN resumed_at TEXT;
