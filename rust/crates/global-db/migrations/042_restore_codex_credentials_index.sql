-- Restore Codex account uniqueness index dropped by migration 039.
-- Columns from migration 036 remain; this only re-enables duplicate-account
-- rejection for OAuth credential adds.
CREATE UNIQUE INDEX IF NOT EXISTS idx_credentials_codex_account
    ON credentials(account_id) WHERE provider = 'codex' AND account_id IS NOT NULL;