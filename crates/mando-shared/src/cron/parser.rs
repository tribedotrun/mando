//! In-house 5-field POSIX cron expression parser.
//!
//! Supports: values, ranges (1-5), steps (*/10), lists (1,3,5).
//! No named ranges (JAN/MON), no @aliases, no year field.

use std::fmt;

/// A parsed cron expression with expanded field values.
#[derive(Debug, Clone)]
pub struct CronExpr {
    pub minutes: Vec<u8>,
    pub hours: Vec<u8>,
    pub days: Vec<u8>,
    pub months: Vec<u8>,
    pub weekdays: Vec<u8>,
}

/// Error type for cron expression parsing.
#[derive(Debug, Clone)]
pub struct CronParseError {
    pub message: String,
}

impl fmt::Display for CronParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cron parse error: {}", self.message)
    }
}

impl std::error::Error for CronParseError {}

/// Parse a single cron field token into a sorted, deduplicated list of values.
///
/// `min`/`max` define the valid range (inclusive) for this field.
fn parse_field(token: &str, min: u8, max: u8) -> Result<Vec<u8>, CronParseError> {
    let mut values = Vec::new();

    for part in token.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(CronParseError {
                message: format!("empty element in field '{token}'"),
            });
        }

        if let Some((base, step_str)) = part.split_once('/') {
            // Step expression: */N or M-N/S
            let step: u8 = step_str.parse().map_err(|e| CronParseError {
                message: format!("invalid step value '{step_str}' in '{token}': {e}"),
            })?;
            if step == 0 {
                return Err(CronParseError {
                    message: format!("step cannot be zero in '{token}'"),
                });
            }

            let (range_min, range_max) = if base == "*" {
                (min, max)
            } else if let Some((lo, hi)) = base.split_once('-') {
                let lo: u8 = lo.parse().map_err(|e| CronParseError {
                    message: format!("invalid range start '{lo}' in '{token}': {e}"),
                })?;
                let hi: u8 = hi.parse().map_err(|e| CronParseError {
                    message: format!("invalid range end '{hi}' in '{token}': {e}"),
                })?;
                (lo, hi)
            } else {
                let start: u8 = base.parse().map_err(|e| CronParseError {
                    message: format!("invalid base '{base}' in '{token}': {e}"),
                })?;
                (start, max)
            };

            if range_min > range_max || range_min < min || range_max > max {
                return Err(CronParseError {
                    message: format!("range {range_min}-{range_max} out of bounds {min}-{max}"),
                });
            }

            let mut v = range_min;
            while v <= range_max {
                values.push(v);
                v = match v.checked_add(step) {
                    Some(next) => next,
                    None => break,
                };
            }
        } else if part == "*" {
            // Wildcard: all values in range.
            for v in min..=max {
                values.push(v);
            }
        } else if let Some((lo, hi)) = part.split_once('-') {
            // Range: M-N
            let lo: u8 = lo.parse().map_err(|_| CronParseError {
                message: format!("invalid range start '{lo}' in '{token}'"),
            })?;
            let hi: u8 = hi.parse().map_err(|_| CronParseError {
                message: format!("invalid range end '{hi}' in '{token}'"),
            })?;
            if lo > hi || lo < min || hi > max {
                return Err(CronParseError {
                    message: format!("range {lo}-{hi} out of bounds {min}-{max}"),
                });
            }
            for v in lo..=hi {
                values.push(v);
            }
        } else {
            // Single value.
            let v: u8 = part.parse().map_err(|e| CronParseError {
                message: format!("invalid value '{part}' in '{token}': {e}"),
            })?;
            if v < min || v > max {
                return Err(CronParseError {
                    message: format!("value {v} out of bounds {min}-{max}"),
                });
            }
            values.push(v);
        }
    }

    values.sort();
    values.dedup();
    Ok(values)
}

impl CronExpr {
    /// Parse a standard 5-field POSIX cron expression.
    ///
    /// Format: `min hour dom month dow`
    pub fn parse(expr: &str) -> Result<CronExpr, CronParseError> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(CronParseError {
                message: format!("expected 5 fields, got {} in '{expr}'", fields.len()),
            });
        }

        let minutes = parse_field(fields[0], 0, 59)?;
        let hours = parse_field(fields[1], 0, 23)?;
        let days = parse_field(fields[2], 1, 31)?;
        let months = parse_field(fields[3], 1, 12)?;
        let weekdays = parse_field(fields[4], 0, 6)?;

        if minutes.is_empty()
            || hours.is_empty()
            || days.is_empty()
            || months.is_empty()
            || weekdays.is_empty()
        {
            return Err(CronParseError {
                message: "one or more fields produced no values".into(),
            });
        }

        Ok(CronExpr {
            minutes,
            hours,
            days,
            months,
            weekdays,
        })
    }

    /// Compute the next matching epoch time (seconds) after `after_epoch_secs`.
    ///
    /// Walks forward minute-by-minute from `after + 60s`. Caps search at
    /// ~4 years (2_102_400 minutes) to prevent infinite loops.
    pub fn next_after(&self, after_epoch_secs: i64) -> Option<i64> {
        // Start from the next whole minute.
        let start = (after_epoch_secs / 60 + 1) * 60;
        let max_iterations: i64 = 4 * 366 * 24 * 60; // ~4 years in minutes

        for i in 0..max_iterations {
            let ts = start + i * 60;
            let (min, hour, mday, month, wday) = epoch_to_fields(ts);

            if self.minutes.contains(&min)
                && self.hours.contains(&hour)
                && self.days.contains(&mday)
                && self.months.contains(&month)
                && self.weekdays.contains(&wday)
            {
                return Some(ts);
            }
        }

        None
    }
}

/// Convert epoch seconds to (minute, hour, day-of-month, month, weekday).
///
/// Uses a manual calculation (no external crate). Weekday: 0=Sunday.
fn epoch_to_fields(epoch: i64) -> (u8, u8, u8, u8, u8) {
    let secs = epoch;
    let minute = ((secs % 3600) / 60) as u8;
    let hour = ((secs % 86400) / 3600) as u8;

    // Days since epoch (1970-01-01, a Thursday = weekday 4).
    let mut days = secs / 86400;
    // Handle negative epochs.
    if secs < 0 && secs % 86400 != 0 {
        days -= 1;
    }
    let wday = ((days % 7 + 4 + 7) % 7) as u8; // 0=Sunday

    // Convert days to year/month/day.
    let (_, month, mday) = days_to_ymd(days);

    (minute, hour, mday as u8, month as u8, wday)
}

use super::days_to_ymd;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_every_5_minutes() {
        let expr = CronExpr::parse("*/5 * * * *").unwrap();
        let expected: Vec<u8> = (0..60).step_by(5).collect();
        assert_eq!(expr.minutes, expected);
        assert_eq!(expr.hours.len(), 24);
        assert_eq!(expr.days.len(), 31);
        assert_eq!(expr.months.len(), 12);
        assert_eq!(expr.weekdays.len(), 7);
    }

    #[test]
    fn parse_weekday_9am() {
        let expr = CronExpr::parse("0 9 * * 1-5").unwrap();
        assert_eq!(expr.minutes, vec![0]);
        assert_eq!(expr.hours, vec![9]);
        assert_eq!(expr.weekdays, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn parse_specific_time() {
        let expr = CronExpr::parse("30 2 1 * *").unwrap();
        assert_eq!(expr.minutes, vec![30]);
        assert_eq!(expr.hours, vec![2]);
        assert_eq!(expr.days, vec![1]);
        assert_eq!(expr.months.len(), 12);
    }

    #[test]
    fn parse_list() {
        let expr = CronExpr::parse("0,15,30,45 * * * *").unwrap();
        assert_eq!(expr.minutes, vec![0, 15, 30, 45]);
    }

    #[test]
    fn parse_invalid_field_count() {
        assert!(CronExpr::parse("* *").is_err());
    }

    #[test]
    fn parse_invalid_value() {
        assert!(CronExpr::parse("60 * * * *").is_err());
    }

    #[test]
    fn parse_invalid_range() {
        assert!(CronExpr::parse("* * 32 * *").is_err());
    }

    #[test]
    fn next_after_known_timestamp() {
        // 2024-01-15 10:00:00 UTC = 1705312800
        let ts = 1705312800_i64;
        // "0 9 * * 1-5" = weekday at 9:00
        let expr = CronExpr::parse("0 9 * * 1-5").unwrap();
        let next = expr.next_after(ts).unwrap();
        // Next weekday 9:00 after Mon Jan 15 10:00 should be Tue Jan 16 9:00
        // 2024-01-16 09:00:00 UTC = 1705395600
        assert_eq!(next, 1705395600);
    }

    #[test]
    fn next_after_every_5_min() {
        // 2024-01-15 10:02:00 UTC = 1705312920
        let ts = 1705312920_i64;
        let expr = CronExpr::parse("*/5 * * * *").unwrap();
        let next = expr.next_after(ts).unwrap();
        // Next */5 after 10:02 should be 10:05 = 1705313100
        assert_eq!(next, 1705313100);
    }
}
