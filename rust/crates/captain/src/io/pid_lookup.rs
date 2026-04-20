//! PID lookup helpers that keep the session registry authoritative.

use crate::Pid;

pub fn prefer_session_pid(session_pid: Option<Pid>, health_pid: Pid) -> Option<Pid> {
    session_pid
        .filter(|pid| pid.as_u32() > 0)
        .or_else(|| (health_pid.as_u32() > 0).then_some(health_pid))
}

pub fn resolve_pid(session_id: &str, worker: &str) -> Option<Pid> {
    prefer_session_pid(
        super::pid_registry::get_pid(session_id),
        super::health_store::get_pid_for_worker(worker),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_session_pid_when_present() {
        assert_eq!(
            prefer_session_pid(Some(Pid::new(42)), Pid::new(7)),
            Some(Pid::new(42))
        );
    }

    #[test]
    fn falls_back_to_health_pid() {
        assert_eq!(prefer_session_pid(None, Pid::new(7)), Some(Pid::new(7)));
    }

    #[test]
    fn ignores_zero_values() {
        assert_eq!(prefer_session_pid(Some(Pid::new(0)), Pid::new(0)), None);
        assert_eq!(prefer_session_pid(None, Pid::new(0)), None);
    }
}
