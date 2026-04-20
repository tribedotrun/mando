//! Single-instance management via PID and port files.
//!
//! Files live in `$MANDO_DATA_DIR` (defaults to `~/.mando/`):
//! - `daemon.pid` — PID of the running daemon
//! - `daemon.port` — port the daemon is listening on (production)
//! - `daemon-dev.port` — port the daemon is listening on (dev mode, `--dev` flag)
//!
//! `check_and_write_pid` refuses to start if another daemon is already running (PID file or port occupant).

use std::fs;
use std::path::PathBuf;

fn pid_path() -> PathBuf {
    global_infra::paths::data_dir().join("daemon.pid")
}

fn port_path(dev: bool) -> PathBuf {
    let name = if dev {
        "daemon-dev.port"
    } else {
        "daemon.port"
    };
    global_infra::paths::data_dir().join(name)
}

/// Ensure no other mando-gw is running, then write our PID.
///
/// If a live mando-gw is found (PID file or port occupant), refuse to start.
/// Lifecycle scripts (`mando-dev stop/restart`) handle killing -- the daemon
/// itself never kills a sibling.
pub fn check_and_write_pid(port: u16) -> anyhow::Result<()> {
    let path = pid_path();

    // Check existing PID file.
    if let Ok(contents) = fs::read_to_string(&path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            if pid != std::process::id() && is_process_alive(pid) {
                anyhow::bail!(
                    "another daemon is already running (pid {pid}). \
                     Stop it first with mando-dev stop"
                );
            }
        }
        // PID file exists but process is dead -- stale file, clean up.
        fs::remove_file(&path).ok();
    }

    // No PID file but port might be occupied (e.g. rm -rf ~/.mando while daemon was running).
    if let Some(pid) = find_port_occupant(port) {
        if pid != std::process::id() {
            anyhow::bail!(
                "port {port} is already occupied (pid {pid}). \
                 Stop it first with mando-dev stop"
            );
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
        if let Err(e) = fs::create_dir_all(parent) {
            global_infra::unrecoverable!("failed to create data dir for port file", e);
        }
    }
    if let Err(e) = fs::write(&path, port.to_string()) {
        global_infra::unrecoverable!("failed to write port file", e);
    }
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
