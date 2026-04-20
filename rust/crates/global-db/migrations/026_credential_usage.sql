-- Per-credential utilization snapshot populated by the proactive usage probe.
-- All timestamps are Unix seconds to match the existing rate_limit_cooldown_until.
-- Utilizations are REAL in [0.0, 1.0]; NULL until the credential is probed once.
ALTER TABLE credentials ADD COLUMN five_hour_utilization REAL;
ALTER TABLE credentials ADD COLUMN five_hour_reset_at INTEGER;
ALTER TABLE credentials ADD COLUMN five_hour_status TEXT;
ALTER TABLE credentials ADD COLUMN seven_day_utilization REAL;
ALTER TABLE credentials ADD COLUMN seven_day_reset_at INTEGER;
ALTER TABLE credentials ADD COLUMN seven_day_status TEXT;
ALTER TABLE credentials ADD COLUMN unified_status TEXT;
ALTER TABLE credentials ADD COLUMN representative_claim TEXT;
ALTER TABLE credentials ADD COLUMN last_probed_at INTEGER;
