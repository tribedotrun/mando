//! PID lookup helpers that keep the session registry authoritative.

pub fn prefer_session_pid(session_pid: Option<u32>, health_pid: u32) -> Option<u32> {
    session_pid
        .filter(|pid| *pid > 0)
        .or_else(|| (health_pid > 0).then_some(health_pid))
}

pub fn resolve_pid(session_id: &str, worker: &str) -> Option<u32> {
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
        assert_eq!(prefer_session_pid(Some(42), 7), Some(42));
    }

    #[test]
    fn falls_back_to_health_pid() {
        assert_eq!(prefer_session_pid(None, 7), Some(7));
    }

    #[test]
    fn ignores_zero_values() {
        assert_eq!(prefer_session_pid(Some(0), 0), None);
        assert_eq!(prefer_session_pid(None, 0), None);
    }
}
