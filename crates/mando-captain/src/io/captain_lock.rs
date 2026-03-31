//! Tick-level file lock for captain execution.

use std::fs;
use std::os::unix::io::AsRawFd;

use anyhow::{bail, Result};

pub(crate) struct CaptainTickLock {
    file: fs::File,
}

impl Drop for CaptainTickLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn try_acquire() -> Result<CaptainTickLock> {
    let lock_path = mando_config::captain_lock_path();
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

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
            mando_types::now_rfc3339()
        )
        .as_bytes(),
    )?;

    Ok(CaptainTickLock { file })
}
