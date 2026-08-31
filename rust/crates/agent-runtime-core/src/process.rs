//! Provider-neutral process isolation and lifecycle helpers.

use anyhow::Result;
use tokio::process::{Child, Command};

/// Daemon environment variables stripped from every agent child process.
///
/// Inheriting these would let a worker silently alter `mando-dev` mode when it
/// invokes project commands from inside its session.
pub const DAEMON_ENV_STRIP: &[&str] = &[
    "MANDO_PROD_MODE",
    "MANDO_APP_MODE",
    "MANDO_SANDBOX",
    "MANDO_ELECTRON_BIN",
    "MANDO_ELECTRON_ENTRYPOINT",
    "MANDO_ELECTRON_INSPECT_PORT",
    "MANDO_ELECTRON_CDP_PORT",
    "MANDO_EXTERNAL_GATEWAY",
];

/// Ownership policy for an agent child handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildLifetime {
    /// The owning runtime explicitly monitors and terminates the child.
    Managed,
    /// Dropping the child handle must terminate the process.
    KillOnDrop,
}

/// Spawn an agent process in its own process group with daemon state removed.
///
/// Every provider uses this boundary so environment isolation, process-group
/// setup, and child-handle ownership cannot drift between adapters.
pub fn spawn_isolated(mut command: Command, lifetime: ChildLifetime) -> std::io::Result<Child> {
    for key in DAEMON_ENV_STRIP {
        command.env_remove(key);
    }
    command.kill_on_drop(lifetime == ChildLifetime::KillOnDrop);

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn()
}

/// Kill an isolated process group: SIGTERM, bounded wait, then SIGKILL.
pub async fn kill_process(pid: global_types::Pid) -> Result<()> {
    if pid.as_u32() == 0 {
        tracing::warn!(
            module = "agent-runtime-core",
            "kill_process called with pid=0, skipping"
        );
        return Ok(());
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-pid.as_i32(), libc::SIGTERM);
    }

    let wait_exit = async {
        while is_process_alive(pid) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    };
    if tokio::time::timeout(std::time::Duration::from_secs(5), wait_exit)
        .await
        .is_ok()
    {
        return Ok(());
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-pid.as_i32(), libc::SIGKILL);
    }
    Ok(())
}

/// Check whether a process id is alive.
pub fn is_process_alive(pid: global_types::Pid) -> bool {
    if pid.as_u32() == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid.as_i32(), 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Read CPU time in seconds for a process via `ps -o cputime=`.
pub async fn get_cpu_time(pid: global_types::Pid) -> Result<f64> {
    let output = tokio::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("cputime=")
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_cputime(&text)
}

fn parse_cputime(value: &str) -> Result<f64> {
    let parts: Vec<&str> = value.split(':').collect();
    match parts.len() {
        3 => {
            let hours: f64 = parts[0].parse()?;
            let minutes: f64 = parts[1].parse()?;
            let seconds: f64 = parts[2].parse()?;
            Ok(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        2 => {
            let minutes: f64 = parts[0].parse()?;
            let seconds: f64 = parts[1].parse()?;
            Ok(minutes * 60.0 + seconds)
        }
        _ => anyhow::bail!("invalid cputime format: {value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cputime_hhmmss() {
        assert!((parse_cputime("01:30:45").unwrap() - 5445.0).abs() < 0.1);
    }

    #[test]
    fn parse_cputime_mmss() {
        assert!((parse_cputime("05:30").unwrap() - 330.0).abs() < 0.1);
    }

    #[test]
    fn pid_zero_not_alive() {
        assert!(!is_process_alive(global_types::Pid::new(0)));
    }
}
