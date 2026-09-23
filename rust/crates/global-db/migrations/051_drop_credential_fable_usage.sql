-- Mando no longer runs Fable, and its usage probe runs the default Opus model,
-- which never reports the Fable weekly bucket. Drop the columns from 048.
ALTER TABLE credentials DROP COLUMN seven_day_fable_utilization;
ALTER TABLE credentials DROP COLUMN seven_day_fable_reset_at;
ALTER TABLE credentials DROP COLUMN seven_day_fable_status;
