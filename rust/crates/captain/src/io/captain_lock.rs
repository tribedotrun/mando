//! Tick-level file lock for captain execution.
//!
//! The `CaptainTickLock` guard releases the advisory BSD lock on drop.
//! `flock(2)` locks are associated with the open file description (not the
//! acquiring thread), so releasing from a different thread is sound — the
//! lock owner from the kernel's perspective is the fd, which is valid for
//! the lifetime of the `fs::File` stored in the guard.

use std::fs;
use std::os::unix::io::AsRawFd;

use anyhow::{bail, Result};

#[must_use = "the captain tick lock is released immediately when dropped"]
pub(crate) struct CaptainTickLock {
    file: fs::File,
}

impl Drop for CaptainTickLock {
    fn drop(&mut self) {
        // SAFETY: `self.file` owns a valid fd for the lifetime of this guard;
        // `flock(LOCK_UN)` on a valid fd is always safe. BSD flock locks are
        // bound to the open file description, not the calling thread, so this
        // is sound even if the guard is dropped on a tokio worker thread
        // different from the one that called `try_acquire`.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn try_acquire() -> Result<CaptainTickLock> {
    let lock_path = crate::config::captain_lock_path();
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    // SAFETY: `file` owns a valid fd; `flock` with `LOCK_EX | LOCK_NB` is safe
    // and non-blocking — a return value of non-zero indicates the lock is held
    // elsewhere.
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        bail!("captain tick lock is already held: {}", lock_path.display());
    }

    file.set_len(0)?;
    use std::io::Write;
    file.write_all(
        format!(
            "{{\"pid\":{},\"acquired_at\":\"{}\"}}",
            std::process::id(),
            global_types::now_rfc3339()
        )
        .as_bytes(),
    )?;

    Ok(CaptainTickLock { file })
}
