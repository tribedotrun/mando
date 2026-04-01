//! Cron API handlers — list, add, remove, toggle, parse schedule.
//!
//! These are pure functions that operate on a `CronService` reference,
//! returning JSON values for the transport layer.

use mando_types::{CronSchedule, ScheduleKind};
use serde_json::{json, Value};

use super::service::CronService;

/// List cron jobs as a JSON value.
pub fn list_cron_jobs(service: &CronService, include_disabled: bool) -> Value {
    let jobs = service.list_jobs(include_disabled);
    let entries: Vec<Value> = jobs.iter().map(|j| job_to_json(j)).collect();
    json!({
        "jobs": entries,
        "count": entries.len(),
    })
}

/// Add a cron job. Returns the created job as JSON.
pub async fn add_cron_job(
    service: &mut CronService,
    id: String,
    name: String,
    schedule: CronSchedule,
    message: String,
    now_ms: i64,
) -> Result<Value, String> {
    let job = service.add_job(id, name, schedule, message, now_ms).await?;
    Ok(job_to_json(&job))
}

/// Remove a cron job. Returns `{"removed": true, "id": "..."}`.
pub async fn remove_cron_job(service: &mut CronService, id: &str) -> Result<Value, String> {
    let removed = service.remove_job(id).await?;
    Ok(json!({
        "removed": removed,
        "id": id,
    }))
}

/// Toggle a cron job enabled/disabled. Returns the updated job as JSON.
pub async fn toggle_cron_job(
    service: &mut CronService,
    id: &str,
    enabled: bool,
) -> Result<Value, String> {
    match service.toggle_job(id, enabled).await? {
        Some(job) => Ok(job_to_json(job)),
        None => Err(format!("job not found: {id}")),
    }
}

/// Parse a schedule kind + value string into a `CronSchedule`.
///
/// - `"every"` + `"30m"` / `"1h"` / `"5s"` / `"2d"` -> `every_ms`
/// - `"cron"` + cron expression -> `expr`
/// - `"at"` + ISO datetime string -> `at_ms`
pub fn parse_schedule(kind: &str, value: &str) -> Result<CronSchedule, String> {
    match kind {
        "every" => {
            let trimmed = value.trim();
            let (num_str, unit) = split_duration(trimmed)?;
            let num: i64 = num_str
                .parse()
                .map_err(|_| format!("invalid number '{num_str}' in '{value}'"))?;
            if num <= 0 {
                return Err(format!("duration must be positive, got {num}"));
            }
            let multiplier = match unit {
                "s" => 1_000,
                "m" => 60_000,
                "h" => 3_600_000,
                "d" => 86_400_000,
                _ => return Err(format!("unknown unit '{unit}' (use s/m/h/d)")),
            };
            Ok(CronSchedule {
                kind: ScheduleKind::Every,
                every_ms: Some(num * multiplier),
                ..CronSchedule::default()
            })
        }
        "cron" => Ok(CronSchedule {
            kind: ScheduleKind::Cron,
            expr: Some(value.trim().to_string()),
            ..CronSchedule::default()
        }),
        "at" => {
            let ms = parse_iso_to_ms(value.trim())?;
            Ok(CronSchedule {
                kind: ScheduleKind::At,
                at_ms: Some(ms),
                ..CronSchedule::default()
            })
        }
        _ => Err(format!("unknown schedule kind: '{kind}'")),
    }
}

// ---------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------

/// Split "30m" into ("30", "m").
fn split_duration(s: &str) -> Result<(&str, &str), String> {
    // Find where the digits end.
    let boundary = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("no unit in duration '{s}' (use e.g. 30m, 1h, 5s)"))?;
    let num = &s[..boundary];
    let unit = &s[boundary..];
    if num.is_empty() {
        return Err(format!("no number in duration '{s}'"));
    }
    Ok((num, unit))
}

/// Minimal ISO 8601 datetime parser -> epoch milliseconds.
///
/// Supports `YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS+00:00`.
fn parse_iso_to_ms(s: &str) -> Result<i64, String> {
    // Strip trailing 'Z' or timezone offset for simplicity.
    let clean = s
        .trim_end_matches('Z')
        .trim_end_matches("+00:00")
        .trim_end_matches("+0000");

    // Split on 'T' or ' '.
    let (date_part, time_part) = if let Some(pos) = clean.find('T') {
        (&clean[..pos], &clean[pos + 1..])
    } else if let Some(pos) = clean.find(' ') {
        (&clean[..pos], &clean[pos + 1..])
    } else {
        return Err(format!("cannot parse datetime '{s}'"));
    };

    let date_fields: Vec<&str> = date_part.split('-').collect();
    if date_fields.len() != 3 {
        return Err(format!("invalid date in '{s}'"));
    }
    let year: i32 = date_fields[0]
        .parse()
        .map_err(|_| format!("invalid year in '{s}'"))?;
    let month: u32 = date_fields[1]
        .parse()
        .map_err(|_| format!("invalid month in '{s}'"))?;
    let day: u32 = date_fields[2]
        .parse()
        .map_err(|_| format!("invalid day in '{s}'"))?;

    let time_fields: Vec<&str> = time_part.split(':').collect();
    if time_fields.len() < 2 {
        return Err(format!("invalid time in '{s}'"));
    }
    let hour: u32 = time_fields[0]
        .parse()
        .map_err(|_| format!("invalid hour in '{s}'"))?;
    let minute: u32 = time_fields[1]
        .parse()
        .map_err(|_| format!("invalid minute in '{s}'"))?;
    let second: u32 = if time_fields.len() > 2 {
        // Handle fractional seconds by taking only the integer part.
        let sec_str = time_fields[2].split('.').next().unwrap_or("0");
        sec_str
            .parse()
            .map_err(|_| format!("invalid second in '{s}'"))?
    } else {
        0
    };

    // Convert to epoch using manual calculation.
    let epoch_days = ymd_to_days(year, month, day);
    let epoch_secs =
        epoch_days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64;
    Ok(epoch_secs * 1000)
}

use super::ymd_to_days;

fn job_to_json(job: &mando_types::CronJob) -> Value {
    json!({
        "id": job.id,
        "name": job.name,
        "enabled": job.enabled,
        "schedule_kind": job.schedule.kind,
        "message": job.payload.message,
        "next_run_at_ms": job.state.next_run_at_ms,
        "last_run_at_ms": job.state.last_run_at_ms,
        "last_status": job.state.last_status,
        "job_type": job.job_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_every_30m() {
        let s = parse_schedule("every", "30m").unwrap();
        assert_eq!(s.kind, ScheduleKind::Every);
        assert_eq!(s.every_ms, Some(30 * 60_000));
    }

    #[test]
    fn parse_every_1h() {
        let s = parse_schedule("every", "1h").unwrap();
        assert_eq!(s.every_ms, Some(3_600_000));
    }

    #[test]
    fn parse_every_5s() {
        let s = parse_schedule("every", "5s").unwrap();
        assert_eq!(s.every_ms, Some(5_000));
    }

    #[test]
    fn parse_every_2d() {
        let s = parse_schedule("every", "2d").unwrap();
        assert_eq!(s.every_ms, Some(2 * 86_400_000));
    }

    #[test]
    fn parse_cron_expr() {
        let s = parse_schedule("cron", "*/5 * * * *").unwrap();
        assert_eq!(s.kind, ScheduleKind::Cron);
        assert_eq!(s.expr.as_deref(), Some("*/5 * * * *"));
    }

    #[test]
    fn parse_at_iso() {
        let s = parse_schedule("at", "2024-01-15T10:00:00Z").unwrap();
        assert_eq!(s.kind, ScheduleKind::At);
        // 2024-01-15T10:00:00Z = 1705312800 seconds
        assert_eq!(s.at_ms, Some(1_705_312_800_000));
    }

    #[test]
    fn parse_unknown_kind_errors() {
        assert!(parse_schedule("bogus", "value").is_err());
    }
}
