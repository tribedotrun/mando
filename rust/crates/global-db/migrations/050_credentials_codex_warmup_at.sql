-- Unix seconds of the last automatic or manual Codex usage warm-up: a dummy
-- `codex exec` prompt that starts an idle credential's rolling rate-limit
-- windows so the reset clock is already ticking before real work lands.
-- NULL until the first warm-up.
ALTER TABLE credentials ADD COLUMN codex_warmup_at INTEGER;
