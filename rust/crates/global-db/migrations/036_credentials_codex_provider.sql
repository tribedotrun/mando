-- Codex provider for credentials (PR #1006).
-- Adds the columns needed to store a second auth provider alongside Claude
-- OAuth tokens. Existing rows stay valid via the `provider` default.
ALTER TABLE credentials ADD COLUMN provider TEXT NOT NULL DEFAULT 'claude'
    CHECK (provider IN ('claude', 'codex'));
ALTER TABLE credentials ADD COLUMN refresh_token TEXT;
ALTER TABLE credentials ADD COLUMN id_token TEXT;
ALTER TABLE credentials ADD COLUMN account_id TEXT;
ALTER TABLE credentials ADD COLUMN plan_type TEXT;
ALTER TABLE credentials ADD COLUMN credits_balance TEXT;
ALTER TABLE credentials ADD COLUMN credits_unlimited INTEGER NOT NULL DEFAULT 0
    CHECK (credits_unlimited IN (0, 1));

-- Distinct chatgpt account uniqueness for Codex rows. Multiple Codex
-- credentials for different accounts is the whole point; the same account
-- twice is a paste mistake we want the API to reject with a 409.
CREATE UNIQUE INDEX idx_credentials_codex_account
    ON credentials(account_id) WHERE provider = 'codex' AND account_id IS NOT NULL;
