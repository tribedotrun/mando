//! Cron scheduling subsystem — parser, scheduler, service, store, and API.

pub mod api;
pub mod parser;
pub mod scheduler;
pub mod service;
pub mod store;

/// Leap year check (Gregorian calendar).
pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Number of days in the given year.
pub fn days_in_year(y: i32) -> i64 {
    if is_leap(y) {
        366
    } else {
        365
    }
}

/// Month-day table for a given year (accounting for leap).
pub fn month_days(y: i32) -> [i64; 12] {
    if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    }
}

/// Convert (year, month 1-12, day 1-31) to days since 1970-01-01.
pub fn ymd_to_days(year: i32, month: u32, day: u32) -> i64 {
    let mut days: i64 = 0;
    for y in 1970..year {
        days += days_in_year(y);
    }
    let md = month_days(year);
    for d in md.iter().take(month as usize - 1) {
        days += *d;
    }
    days + day as i64 - 1
}

/// Convert days since 1970-01-01 to (year, month 1-12, day 1-31).
pub fn days_to_ymd(mut days: i64) -> (i32, i32, i32) {
    let mut y = 1970;
    loop {
        let yd = days_in_year(y);
        if days < yd {
            break;
        }
        days -= yd;
        y += 1;
    }

    let md = month_days(y);
    let mut month = 0;
    for (i, &d) in md.iter().enumerate() {
        if days < d {
            month = i as i32 + 1;
            break;
        }
        days -= d;
    }

    (y, month, days as i32 + 1)
}
