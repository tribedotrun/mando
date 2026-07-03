-- Track credential token freshness separately from usage/UI metadata updates.
ALTER TABLE credentials ADD COLUMN token_updated_at INTEGER;

UPDATE credentials
SET token_updated_at = CAST(strftime('%s', updated_at) AS INTEGER)
WHERE token_updated_at IS NULL;
