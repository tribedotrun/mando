//! Single-instance management via PID and port files.
//!
//! Files live in `$MANDO_DATA_DIR` (defaults to `~/.mando/`):
//! - `daemon.pid` — PID of the running daemon
//! - `daemon.port` — port the daemon is listening on (production)
//! - `daemon-dev.port` — port the daemon is listening on (dev mode, `--dev` flag)
//!
//! `check_and_write_pid` fails if another daemon is already running.

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

/// Check whether another daemon is already running. If so, return an error.
/// Otherwise, write our PID to the PID file.
pub fn check_and_write_pid() -> anyhow::Result<()> {
    let path = pid_path();

    // Check existing PID file.
    if let Ok(contents) = fs::read_to_string(&path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            // Skip if the PID matches our own (shell wrapper may pre-write our PID).
            if pid != std::process::id() && is_process_alive(pid) {
                anyhow::bail!(
                    "another mando-gw is already running (pid {pid}). \
                     Remove {} if this is stale.",
                    path.display()
                );
            }
        }
        // Stale PID file — remove it.
        fs::remove_file(&path).ok();
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
    fs::remove_file(pid_path()).ok();
    fs::remove_file(port_path(dev)).ok();
}

/// Check if a process with the given PID is still alive.
fn is_process_alive(pid: u32) -> bool {
    // `kill -0 <pid>` checks existence without sending a signal.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
