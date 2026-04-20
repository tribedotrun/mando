use anyhow::{anyhow, Context};
use tokio::process::{Child, Command};

use crate::types::UiLaunchSpec;

pub fn spawn_process(spec: &UiLaunchSpec) -> anyhow::Result<(i32, Child)> {
    let mut command = Command::new(&spec.exec_path);
    command.args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }

    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn ui process {}", spec.exec_path))?;
    let pid = child
        .id()
        .map(|value| value as i32)
        .ok_or_else(|| anyhow!("ui spawn returned no pid"))?;
    Ok((pid, child))
}

pub fn terminate_pid(pid: i32) -> anyhow::Result<()> {
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(anyhow!(
            "failed to terminate ui pid {pid}: {}",
            std::io::Error::last_os_error()
        ))
    }
}

pub fn is_pid_alive(pid: i32) -> bool {
    let rc = unsafe { libc::kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}

pub fn force_kill_pid(pid: i32) {
    unsafe { libc::kill(pid, libc::SIGKILL) };
}
