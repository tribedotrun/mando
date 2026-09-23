-- Restore the Fable weekly window dropped in 051. A display-only Fable probe
-- now fills it; credential selection still keys off the default-model probe.
ALTER TABLE credentials ADD COLUMN seven_day_fable_utilization REAL;
ALTER TABLE credentials ADD COLUMN seven_day_fable_reset_at INTEGER;
ALTER TABLE credentials ADD COLUMN seven_day_fable_status TEXT;
