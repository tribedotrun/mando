//! Per-item operation locks — prevent concurrent mutations to the same task.
//!
//! Uses file-based exclusive locks (`flock(LOCK_EX | LOCK_NB)`) with holder tracking
//! JSON and stale PID detection. Lock files live at `~/.mando/state/item-locks/<item_id>.lock`.

use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use anyhow::{bail, Result};

/// Lock file content — tracks who holds the lock.
#[derive(serde::Serialize, serde::Deserialize)]
struct LockHolder {
    pid: u32,
    operation: String,
    acquired_at: String,
}

/// An acquired per-item operation lock. Releases on drop.
pub struct ItemLock {
    file: fs::File,
    path: PathBuf,
}

impl Drop for ItemLock {
    fn drop(&mut self) {
        // Release flock.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
        // Remove lock file (best-effort).
        fs::remove_file(&self.path).ok();
    }
}

/// Acquire an exclusive per-item lock. Returns an error if the item is already locked
/// by a live process.
pub(crate) fn acquire_item_lock(item_id: &str, operation: &str) -> Result<ItemLock> {
    let lock_dir = lock_dir();
    fs::create_dir_all(&lock_dir)?;

    let lock_path = lock_dir.join(format!("{item_id}.lock"));

    // Check for stale lock.
    if lock_path.exists() {
        if let Ok(content) = fs::read_to_string(&lock_path) {
            if let Ok(holder) = serde_json::from_str::<LockHolder>(&content) {
                if is_pid_alive(holder.pid) {
                    bail!(
                        "item {item_id} locked by PID {} (operation: {}, since {})",
                        holder.pid,
                        holder.operation,
                        holder.acquired_at
                    );
                }
                // Stale lock — PID is dead. Clean it up.
                tracing::info!(
                    module = "item-lock",
                    item_id = %item_id,
                    pid = holder.pid,
                    "cleaning stale lock, PID dead"
                );
                fs::remove_file(&lock_path).ok();
            }
        }
    }

    // Create/open lock file.
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&lock_path)?;

    // Try non-blocking exclusive flock.
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        bail!("item {item_id} is locked by another process (flock failed)");
    }

    // Write holder info.
    let holder = LockHolder {
        pid: std::process::id(),
        operation: operation.to_string(),
        acquired_at: global_types::now_rfc3339(),
    };
    let mut f = &file;
    serde_json::to_writer(&mut f, &holder)?;

    Ok(ItemLock {
        file,
        path: lock_path,
    })
}

/// Clean up stale locks (dead PIDs) — called at tick start.
pub(crate) fn clean_stale_locks() {
    let lock_dir = lock_dir();
    let entries = match fs::read_dir(&lock_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(module = "item-lock", error = %e, path = %lock_dir.display(), "cannot read lock dir");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(module = "item-lock", error = %e, path = %path.display(), "cannot read lock file");
                continue;
            }
        };
        let holder: LockHolder = match serde_json::from_str(&content) {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(module = "item-lock", error = %e, path = %path.display(), "lock file JSON invalid");
                continue;
            }
        };
        if !is_pid_alive(holder.pid) {
            tracing::info!(
                module = "item-lock",
                path = %path.display(),
                pid = holder.pid,
                "removing stale lock, PID dead"
            );
            if let Err(e) = fs::remove_file(&path) {
                tracing::warn!(module = "item-lock", path = %path.display(), error = %e, "failed to remove stale lock file");
            }
        }
    }
}

fn lock_dir() -> PathBuf {
    global_infra::paths::state_dir().join("item-locks")
}

fn is_pid_alive(pid: u32) -> bool {
    // kill(pid, 0) checks if process exists without sending a signal.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let lock = acquire_item_lock("test-lock-1", "test-op").unwrap();
        assert!(lock.path.exists());
        drop(lock);
        // Lock file should be cleaned up.
    }

    #[test]
    fn double_lock_fails() {
        let lock1 = acquire_item_lock("test-lock-2", "op-a").unwrap();
        let result = acquire_item_lock("test-lock-2", "op-b");
        assert!(result.is_err());
        drop(lock1);
    }

    #[test]
    fn clean_stale_locks_removes_dead() {
        let lock_dir = lock_dir();
        fs::create_dir_all(&lock_dir).unwrap();
        let path = lock_dir.join("stale-test.lock");
        let holder = LockHolder {
            pid: 999999, // Almost certainly dead.
            operation: "test".into(),
            acquired_at: "2024-01-01T00:00:00Z".into(),
        };
        fs::write(&path, serde_json::to_string(&holder).unwrap()).unwrap();

        clean_stale_locks();

        assert!(!path.exists(), "stale lock should be removed");
    }
}
