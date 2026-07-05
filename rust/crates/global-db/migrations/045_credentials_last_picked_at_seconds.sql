-- Unify last_picked_at to Unix seconds. `record_codex_pick` previously
-- stamped Unix milliseconds (inconsistent with token_updated_at, which is
-- seconds); the writer now switches to unix_timestamp(). Convert existing
-- values so ordering by last_picked_at stays correct after the writer change.
UPDATE credentials SET last_picked_at = last_picked_at / 1000 WHERE last_picked_at IS NOT NULL;
