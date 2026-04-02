//! Single-instance management via PID and port files.
//!
//! Files live in `$MANDO_DATA_DIR` (defaults to `~/.mando/`):
//! - `daemon.pid` — PID of the running daemon
//! - `daemon.port` — port the daemon is listening on (production)
//! - `daemon-dev.port` — port the daemon is listening on (dev mode, `--dev` flag)
//!
//! `check_and_write_pid` kills stale daemons (PID file or port occupant) before writing our PID.

use std::fs;
use std::path::PathBuf;

fn pid_path() -> PathBuf {
    mando_config::data_dir().join("daemon.pid")
}

fn port_path(dev: bool) -> PathBuf {
    let name = if dev {
        "daemon-dev.port"
    } else {
        "daemon.port"
    };
    mando_config::data_dir().join(name)
}

/// Ensure no other mando-gw is running, then write our PID.
///
/// If a stale mando-gw process is found (PID file or port occupant), kill it.
/// If the process on the port is NOT mando-gw, bail — don't kill unrelated processes.
pub fn check_and_write_pid(port: u16) -> anyhow::Result<()> {
    let path = pid_path();

    // Check existing PID file.
    if let Ok(contents) = fs::read_to_string(&path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            if pid != std::process::id() && is_process_alive(pid) {
                if is_mando_process(pid) {
                    eprintln!("killing stale daemon (pid {pid}) before starting");
                    kill_process(pid);
                } else {
                    anyhow::bail!(
                        "PID file points to non-mando process (pid {pid}). \
                         Remove {} manually.",
                        path.display()
                    );
                }
            }
        }
        fs::remove_file(&path).ok();
    }

    // No PID file but port might be occupied (e.g. rm -rf ~/.mando while daemon was running).
    if let Some(pid) = find_port_occupant(port) {
        if pid != std::process::id() {
            if is_mando_process(pid) {
                eprintln!("killing stale daemon on port {port} (pid {pid})");
                kill_process(pid);
            } else {
                anyhow::bail!(
                    "port {port} is occupied by another process (pid {pid}, not mando-gw)"
                );
            }
        }
    }

    // Write our PID.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, std::process::id().to_string())?;
    Ok(())
}

/// Write the port file so clients can discover which port the daemon bound to.
pub fn write_port_file(port: u16, dev: bool) {
    let path = port_path(dev);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create data dir");
    }
    fs::write(&path, port.to_string()).expect("failed to write port file");
}

/// Remove PID and port files on shutdown.
pub fn cleanup_files(dev: bool) {
    if let Err(e) = fs::remove_file(pid_path()) {
        tracing::debug!(error = %e, "failed to remove PID file on shutdown");
    }
    if let Err(e) = fs::remove_file(port_path(dev)) {
        tracing::debug!(error = %e, "failed to remove port file on shutdown");
    }
}

/// Check if a process with the given PID is still alive.
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Send SIGTERM, wait briefly, then SIGKILL if still alive.
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    std::thread::sleep(std::time::Duration::from_millis(500));
    if is_process_alive(pid) {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Check if a PID belongs to a mando-gw process (or "Mando Daemon" in prod).
fn is_mando_process(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(o) => {
            let comm = String::from_utf8_lossy(&o.stdout);
            let name = comm.trim();
            name.contains("mando-gw") || name.contains("Mando Daemon")
        }
        Err(_) => false,
    }
}

/// Find the PID of a process listening on a TCP port (macOS lsof).
fn find_port_occupant(port: u16) -> Option<u32> {
    let output = std::process::Command::new("lsof")
        .args(["-iTCP", &format!(":{port}"), "-sTCP:LISTEN", "-t"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().lines().next()?.parse().ok()
}
