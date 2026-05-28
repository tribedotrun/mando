-- Remove the short-lived Codex account credential feature.
-- Codex task/session/terminal support continues to use the active local
-- Codex CLI login; the credentials table is Claude-only again.
UPDATE cc_sessions
SET credential_id = NULL
WHERE credential_id IN (
    SELECT id FROM credentials WHERE provider = 'codex'
);

DELETE FROM credentials WHERE provider = 'codex';

DROP INDEX IF EXISTS idx_credentials_codex_account;
