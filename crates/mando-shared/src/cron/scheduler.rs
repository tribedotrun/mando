//! Pure scheduling logic: compute next runs, validate schedules.
//!
//! No I/O, no async — pure functions only.

use mando_types::{CronSchedule, ScheduleKind};

use super::parser::CronExpr;

/// Compute the next run time in milliseconds for a given schedule.
///
/// - `Every`: `now_ms + every_ms`
/// - `At`: `at_ms` if in the future, else `None`
/// - `Cron`: parse expression, walk forward from `now_ms`
pub fn compute_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    match schedule.kind {
        ScheduleKind::At => {
            let at = schedule.at_ms?;
            if at > now_ms {
                Some(at)
            } else {
                None
            }
        }
        ScheduleKind::Every => {
            let every = schedule.every_ms?;
            if every <= 0 {
                return None;
            }
            Some(now_ms + every)
        }
        ScheduleKind::Cron => {
            let expr_str = schedule.expr.as_deref()?;
            let parsed = CronExpr::parse(expr_str).ok()?;
            let now_secs = now_ms / 1000;
            let next_secs = parsed.next_after(now_secs)?;
            Some(next_secs * 1000)
        }
    }
}

/// Validate a schedule for adding a new cron job.
pub fn validate_schedule(schedule: &CronSchedule) -> Result<(), String> {
    match schedule.kind {
        ScheduleKind::At => {
            if schedule.at_ms.is_none() {
                return Err("'at' schedule requires at_ms".into());
            }
        }
        ScheduleKind::Every => match schedule.every_ms {
            Some(ms) if ms > 0 => {}
            _ => return Err("'every' schedule requires positive every_ms".into()),
        },
        ScheduleKind::Cron => {
            let expr_str = schedule
                .expr
                .as_deref()
                .ok_or("'cron' schedule requires expr")?;
            CronExpr::parse(expr_str).map_err(|e| format!("invalid cron expression: {e}"))?;
        }
    }

    // tz only valid for cron kind
    if schedule.tz.is_some() && schedule.kind != ScheduleKind::Cron {
        return Err("tz can only be used with cron schedules".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_every(ms: i64) -> CronSchedule {
        CronSchedule {
            kind: ScheduleKind::Every,
            every_ms: Some(ms),
            ..CronSchedule::default()
        }
    }

    fn make_at(ms: i64) -> CronSchedule {
        CronSchedule {
            kind: ScheduleKind::At,
            at_ms: Some(ms),
            ..CronSchedule::default()
        }
    }

    fn make_cron(expr: &str) -> CronSchedule {
        CronSchedule {
            kind: ScheduleKind::Cron,
            expr: Some(expr.into()),
            ..CronSchedule::default()
        }
    }

    #[test]
    fn every_schedule_adds_interval() {
        let now = 1_000_000;
        let schedule = make_every(60_000);
        assert_eq!(compute_next_run(&schedule, now), Some(1_060_000));
    }

    #[test]
    fn every_schedule_zero_returns_none() {
        let schedule = make_every(0);
        assert_eq!(compute_next_run(&schedule, 1000), None);
    }

    #[test]
    fn at_schedule_future_returns_value() {
        let now = 1_000_000;
        let schedule = make_at(2_000_000);
        assert_eq!(compute_next_run(&schedule, now), Some(2_000_000));
    }

    #[test]
    fn at_schedule_past_returns_none() {
        let now = 3_000_000;
        let schedule = make_at(2_000_000);
        assert_eq!(compute_next_run(&schedule, now), None);
    }

    #[test]
    fn cron_schedule_delegates_to_parser() {
        // "0 9 * * *" = every day at 9:00
        let schedule = make_cron("0 9 * * *");
        let now_ms = 1_705_312_800_000; // 2024-01-15 10:00:00 UTC
        let next = compute_next_run(&schedule, now_ms).unwrap();
        // Next 9:00 is 2024-01-16 09:00:00 UTC = 1_705_395_600_000
        assert_eq!(next, 1_705_395_600_000);
    }

    #[test]
    fn cron_schedule_invalid_expr_returns_none() {
        let schedule = make_cron("bad");
        assert_eq!(compute_next_run(&schedule, 1000), None);
    }

    #[test]
    fn validate_every_ok() {
        assert!(validate_schedule(&make_every(60_000)).is_ok());
    }

    #[test]
    fn validate_every_zero_err() {
        assert!(validate_schedule(&make_every(0)).is_err());
    }

    #[test]
    fn validate_at_ok() {
        assert!(validate_schedule(&make_at(1_000_000)).is_ok());
    }

    #[test]
    fn validate_cron_ok() {
        assert!(validate_schedule(&make_cron("*/5 * * * *")).is_ok());
    }
}
