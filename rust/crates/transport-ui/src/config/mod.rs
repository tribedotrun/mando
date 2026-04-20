use std::time::Duration;

pub(crate) const MAX_SPAWN_FAILURES: u32 = 5;
pub(crate) const UPDATE_GRACE_PERIOD: Duration = Duration::from_secs(10);
pub(crate) const AUTO_REGISTER_WAIT: Duration = Duration::from_secs(3);
