pub const MAX_BACKOFF_SECS: u64 = 60;
pub const DEGRADED_FAILURE_COUNT: u32 = 5;
pub const DEGRADED_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
