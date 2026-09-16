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
