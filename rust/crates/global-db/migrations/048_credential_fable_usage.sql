-- Claude's included weekly Fable allowance, reported separately from the
-- ordinary weekly window. NULL until a probe reports this model window.
ALTER TABLE credentials ADD COLUMN seven_day_fable_utilization REAL;
ALTER TABLE credentials ADD COLUMN seven_day_fable_reset_at INTEGER;
ALTER TABLE credentials ADD COLUMN seven_day_fable_status TEXT;
