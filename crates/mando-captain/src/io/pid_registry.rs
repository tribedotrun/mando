//! Session PID registry at `~/.mando/state/session-pids.json`.
//!
//! Single authority for all CC session PIDs. Ephemeral runtime state:
//! written on spawn/resume, removed on terminate, cleaned on startup.
//!
//! All load-modify-save operations run under an exclusive `flock` on
//! `session-pids.lock` to prevent TOCTOU races when multiple tasks mutate the
//! registry concurrently.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use anyhow::{Context, Result};
use mando_shared::load_json_file;
use mando_types::Pid;

type PidMap = HashMap<String, Pid>;

fn registry_path() -> PathBuf {
    mando_config::state_dir().join("session-pids.json")
}

fn lock_path() -> PathBuf {
    mando_config::state_dir().join("session-pids.lock")
}

/// RAII flock guard over `session-pids.lock`. Blocks until the lock is
/// acquired; releases on drop.
struct RegistryLock {
    file: fs::File,
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn acquire_lock() -> Result<RegistryLock> {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("pid_registry: create state dir {}", parent.display()))?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("pid_registry: open lock file {}", path.display()))?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if ret != 0 {
        anyhow::bail!("pid_registry: flock LOCK_EX failed on {}", path.display());
    }
    Ok(RegistryLock { file })
}

/// Load the PID map from disk. A missing file is a healthy fresh state and
/// returns an empty map; any other error (permission denied, corrupt JSON)
/// propagates so callers can fail the spawn/terminate instead of silently
/// losing worker PIDs.
fn load() -> Result<PidMap> {
    let path = registry_path();
    if !path.exists() {
        return Ok(PidMap::default());
    }
    Ok(load_json_file(&path, "pid_registry")?)
}

/// Atomic save (temp file + rename). Assumes the caller already holds the
/// registry flock.
///
/// Uses a per-call unique temp name (PID + monotonic counter + nanos) for
/// defense-in-depth against cross-process races during daemon restart: the
/// flock is per-fd, so an old daemon and a freshly spawned one could briefly
/// both hold their own lock instances and race on a fixed temp name. Unique
/// names ensure each writer has its own temp file and a failed rename only
/// leaves that writer's file behind (cleaned up on the error path below).
fn save(map: &PidMap) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    let path = registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("pid_registry: create state dir {}", parent.display()))?;
    }
    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.tmp.{}.{}.{}", std::process::id(), seq, nanos,));
    let mut f = fs::File::create(&tmp)
        .with_context(|| format!("pid_registry: create {}", tmp.display()))?;
    serde_json::to_writer_pretty(&mut f, map)
        .context("pid_registry: serialize session-pids map")?;
    f.flush().context("pid_registry: flush tmp file")?;
    f.sync_all().context("pid_registry: fsync tmp file")?;
    drop(f);
    fs::rename(&tmp, &path).with_context(|| {
        let _ = fs::remove_file(&tmp);
        format!(
            "pid_registry: rename {} -> {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

/// Record a session's PID. Overwrites any previous entry for this session.
pub fn register(session_id: &str, pid: Pid) -> Result<()> {
    let _guard = acquire_lock()?;
    let mut map = load()?;
    map.insert(session_id.to_string(), pid);
    save(&map)
}

/// Remove a session from the registry.
pub fn unregister(session_id: &str) -> Result<()> {
    let _guard = acquire_lock()?;
    let mut map = load()?;
    if map.remove(session_id).is_some() {
        save(&map)?;
    }
    Ok(())
}

/// Look up the PID for a session. No lock needed for a single read, but the
/// view may race with a concurrent writer; callers should treat the result
/// as advisory. On load failure logs at error level and returns None so
/// callers can decide to fail the operation that needed the PID.
pub fn get_pid(session_id: &str) -> Option<Pid> {
    match load() {
        Ok(map) => map.get(session_id).copied(),
        Err(e) => {
            tracing::error!(module = "pid_registry", session_id, error = %e, "pid_registry load failed");
            None
        }
    }
}

/// Remove entries for dead processes. Call on daemon startup.
pub fn cleanup_dead() -> Result<()> {
    let _guard = acquire_lock()?;
    let mut map = load()?;
    let before = map.len();
    map.retain(|_, pid| mando_cc::is_process_alive(*pid));
    let removed = before - map.len();
    if removed > 0 {
        tracing::info!(
            module = "pid_registry",
            removed,
            remaining = map.len(),
            "startup: cleaned dead PIDs"
        );
        save(&map)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test using the raw map functions directly to avoid hitting the
    /// production registry path.
    #[test]
    fn register_and_get_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), Pid::new(1234));
        assert_eq!(map.get("s1").copied(), Some(Pid::new(1234)));
    }

    #[test]
    fn overwrite_on_resume_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), Pid::new(1000));
        map.insert("s1".into(), Pid::new(2000));
        assert_eq!(map.get("s1").copied(), Some(Pid::new(2000)));
    }

    #[test]
    fn unregister_removes_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), Pid::new(1234));
        map.remove("s1");
        assert_eq!(map.get("s1").copied(), None);
    }

    #[test]
    fn get_missing_returns_none_via_map() {
        let map = PidMap::new();
        assert_eq!(map.get("nonexistent").copied(), None);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("mando-pid-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("session-pids.json");

        let mut map = PidMap::new();
        map.insert("s1".into(), Pid::new(42));
        map.insert("s2".into(), Pid::new(99));

        // Write directly to temp path.
        std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap()).unwrap();

        // Read back.
        let loaded: PidMap = load_json_file(&path, "test").unwrap_or_default();
        assert_eq!(loaded.get("s1").copied(), Some(Pid::new(42)));
        assert_eq!(loaded.get("s2").copied(), Some(Pid::new(99)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
