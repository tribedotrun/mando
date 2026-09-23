-- External CLI processes are independent of captain-owned session lifecycle.
CREATE TABLE claude_launch_leases (
    launch_id TEXT PRIMARY KEY,
    credential_id INTEGER REFERENCES credentials(id) ON DELETE CASCADE,
    session_id TEXT,
    pid INTEGER,
    cwd TEXT,
    model TEXT,
    expires_at INTEGER NOT NULL,
    pending_credential_id INTEGER REFERENCES credentials(id) ON DELETE SET NULL,
    pending_expires_at INTEGER
);
CREATE INDEX claude_launch_leases_credential ON claude_launch_leases(credential_id);
CREATE INDEX claude_launch_leases_pending ON claude_launch_leases(pending_credential_id);
-- Persists probe throttling across concurrent requests and daemon restarts.
CREATE TABLE claude_route_probe_attempts (
    credential_id INTEGER NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    model_bucket TEXT NOT NULL,
    attempted_at INTEGER NOT NULL,
    completed_at INTEGER,
    PRIMARY KEY (credential_id, model_bucket)
);

-- A non-Fable probe must not make a cached model-specific window appear fresh.
ALTER TABLE credentials ADD COLUMN fable_last_probed_at INTEGER;
UPDATE credentials SET fable_last_probed_at = last_probed_at WHERE seven_day_fable_utilization IS NOT NULL;
