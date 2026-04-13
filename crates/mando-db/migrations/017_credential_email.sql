-- Credentials table for additional Claude Code OAuth tokens (setup tokens).
-- The host's own Claude Code login is used implicitly when no credentials exist.
CREATE TABLE IF NOT EXISTS credentials (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    label           TEXT    NOT NULL UNIQUE,
    access_token    TEXT    NOT NULL,
    expires_at      INTEGER,           -- Unix ms; NULL = no expiry
    rate_limit_cooldown_until INTEGER,  -- Unix seconds
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Track which credential was assigned to each worker session.
ALTER TABLE cc_sessions ADD COLUMN credential_id INTEGER;
CREATE INDEX idx_cc_sessions_credential ON cc_sessions(credential_id);
