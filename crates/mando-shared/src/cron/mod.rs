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
