mod runtime_policy;

pub(crate) use runtime_policy::{
    degraded_window_exhausted, restart_backoff_secs, telegram_enabled,
};
