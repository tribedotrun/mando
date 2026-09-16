//! Session PID registry at `~/.mando/state/session-pids.json`.
//!
//! Single authority for all CC session PIDs. Ephemeral runtime state:
//! written on spawn/resume, removed on terminate, cleaned on startup.
//!
//! Each entry stores the subprocess PID plus an opaque "start-time
//! fingerprint" captured at registration. On cleanup we re-capture the
//! fingerprint and skip the kill if it differs: a mismatch means the
//! kernel reused the PID and the current process is someone else's.
//! Without this guard, a daemon that was down long enough for PID reuse
//! could signal an unrelated process group.
//!
//! All load-modify-save operations run under an exclusive `flock` on
//! `session-pids.lock` to prevent TOCTOU races when multiple tasks mutate the
//! registry concurrently.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::Pid;
use anyhow::{Context, Result};
use global_infra::load_json_file;

/// Registry entry: the subprocess PID and a fingerprint captured at
/// registration time. The fingerprint is the output of
/// `ps -p <pid> -o lstart=` which is stable for the lifetime of the
/// process and differs across PID reuse.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PidEntry {
    pub pid: Pid,
    /// Process start-time fingerprint. Empty if `ps` was unavailable at
    /// registration time. An empty fingerprint fails the identity check
    /// and causes cleanup to skip the kill.
    pub started_at: String,
}

impl Default for PidEntry {
    fn default() -> Self {
        Self {
            pid: Pid::new(0),
            started_at: String::new(),
        }
    }
}

type PidMap = HashMap<String, PidEntry>;

/// Capture a stable start-time fingerprint for a process. Returns the
/// empty string if the probe fails (process gone, ps missing). Empty
/// fingerprints always fail the identity comparison so cleanup defaults
/// to safe-skip rather than potentially killing an unrelated process.
fn capture_start_fingerprint(pid: Pid) -> String {
    if pid.as_u32() == 0 {
        return String::new();
    }
    match std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("lstart=")
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn registry_path() -> PathBuf {
    global_infra::paths::state_dir().join("session-pids.json")
}

fn lock_path() -> PathBuf {
    global_infra::paths::state_dir().join("session-pids.lock")
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
    Ok(load_json_file(&path)?)
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
        global_infra::best_effort!(fs::remove_file(&tmp), "pid_registry: fs::remove_file(&tmp)");
        format!(
            "pid_registry: rename {} -> {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

/// Record a session's PID. Overwrites any previous entry for this session.
///
/// Captures the process start-time fingerprint at registration so a
/// later cleanup pass can detect PID reuse. Fingerprint capture runs
/// outside the registry lock so we don't block other callers while
/// `ps` executes.
pub fn register(session_id: &str, pid: Pid) -> Result<PidEntry> {
    let started_at = capture_start_fingerprint(pid);
    let entry = PidEntry { pid, started_at };
    let _guard = acquire_lock()?;
    let mut map = load()?;
    map.insert(session_id.to_string(), entry.clone());
    save(&map)?;
    Ok(entry)
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

/// Remove a session from the registry only if it still points at the exact
/// process identity captured at registration. Returns `true` when an entry was
/// removed. Prefer this over PID-only removal for watcher/finalizer cleanup
/// where PID reuse can race a resumed process.
pub fn unregister_entry_if_current(session_id: &str, expected: &PidEntry) -> Result<bool> {
    let _guard = acquire_lock()?;
    let mut map = load()?;
    if map.get(session_id).is_some_and(|entry| entry == expected) {
        map.remove(session_id);
        save(&map)?;
        return Ok(true);
    }
    Ok(false)
}

/// Look up the full process identity for a session. No lock needed for a single
/// read, but the view may race with a concurrent writer; callers should treat
/// the result as a potentially stale snapshot.
pub fn get_entry(session_id: &str) -> Option<PidEntry> {
    match load() {
        Ok(map) => map.get(session_id).cloned(),
        Err(e) => {
            tracing::error!(module = "pid_registry", session_id, error = %e, "pid_registry load failed");
            None
        }
    }
}

/// Look up the PID for a session. No lock needed for a single read, but the
/// view may race with a concurrent writer; callers should treat the result
/// as a potentially stale snapshot. On load failure logs at error level and returns None so
/// callers can decide to fail the operation that needed the PID.
pub fn get_pid(session_id: &str) -> Option<Pid> {
    match load() {
        Ok(map) => map.get(session_id).map(|e| e.pid),
        Err(e) => {
            tracing::error!(module = "pid_registry", session_id, error = %e, "pid_registry load failed");
            None
        }
    }
}

/// Look up the PID for a session with fingerprint verification. Returns
/// `Some(pid)` only if the process is alive AND its start-time fingerprint
/// matches the one stored at registration (proving the kernel has not reused
/// the PID for an unrelated process). Entries whose `ps` probe failed at
/// registration time (empty fingerprint) skip verification and are trusted
/// if alive.
///
/// Use this for kill sites where signalling a wrong process is dangerous.
/// Non-kill lookups (display, health, liveness decisions) should keep using
/// `get_pid()`.
pub fn get_verified_pid(session_id: &str) -> Option<Pid> {
    let entry = match load() {
        Ok(map) => map.get(session_id).cloned(),
        Err(e) => {
            tracing::error!(module = "pid_registry", session_id, error = %e, "pid_registry load failed");
            return None;
        }
    }?;

    if entry.pid.as_u32() == 0 || !global_claude::is_process_alive(entry.pid) {
        return None;
    }

    if entry.started_at.is_empty() {
        return Some(entry.pid);
    }

    let current_fp = capture_start_fingerprint(entry.pid);
    if current_fp == entry.started_at {
        Some(entry.pid)
    } else {
        tracing::info!(
            module = "pid_registry",
            session_id,
            pid = %entry.pid,
            stored_fp = %entry.started_at,
            current_fp = %current_fp,
            "PID reuse detected (fingerprint mismatch); refusing to return PID for kill"
        );
        None
    }
}

/// Startup cleanup: kill any live subprocesses from a prior daemon and
/// empty the registry.
///
/// The previous daemon may have been SIGTERMed (launchd recycle, manual
/// quit, crash) while Claude Code subprocesses were still running. Those
/// subprocesses are now orphans: their stdout is no longer read by any
/// live daemon, so they cannot deliver results or accept control messages.
/// Leaving them running wastes credentials and confuses downstream
/// reconciliation which would observe "PID alive" and incorrectly skip
/// the stuck session.
///
/// Kills run in parallel under a single 6s deadline (one `kill_process`
/// takes up to 5s on its own for the SIGTERM grace window; one extra
/// second covers the post-kill liveness probe and scheduler jitter).
/// After it returns, the invariant is: zero PIDs registered. Any entry
/// that was still alive after its kill attempt is reported via the
/// `failed` count in the terminal log so operators see when the
/// invariant is weaker than "zero live subprocesses."
pub async fn cleanup_on_startup() -> Result<()> {
    const CLEANUP_GRACE: std::time::Duration = std::time::Duration::from_secs(6);

    let map = {
        let _guard = acquire_lock()?;
        load()?
    };
    if map.is_empty() {
        return Ok(());
    }

    let before = map.len();
    // Partition in three passes: already-dead, PID reused (fingerprint
    // mismatch), and genuinely-our-orphan. We only signal the last group.
    let mut to_kill: Vec<(String, Pid)> = Vec::new();
    let mut already_dead: u32 = 0;
    let mut skipped_pid_reuse: u32 = 0;
    for (session_id, entry) in map {
        if entry.pid.as_u32() == 0 || !global_claude::is_process_alive(entry.pid) {
            already_dead += 1;
            continue;
        }
        // PID reuse check: an empty stored fingerprint (ps unavailable
        // at register time) fails closed so we never kill a PID we
        // can't positively identify.
        let current_fp = capture_start_fingerprint(entry.pid);
        if entry.started_at.is_empty() || current_fp != entry.started_at {
            skipped_pid_reuse += 1;
            tracing::warn!(
                module = "pid_registry",
                %session_id,
                pid = %entry.pid,
                stored_fp = %entry.started_at,
                current_fp = %current_fp,
                "pid reuse suspected (fingerprint mismatch); skipping orphan kill"
            );
            continue;
        }
        to_kill.push((session_id, entry.pid));
    }

    let kill_count = to_kill.len();
    let kills = to_kill.into_iter().map(|(session_id, pid)| async move {
        global_infra::best_effort!(
            global_claude::kill_process(pid).await,
            "pid_registry: global_claude::kill_process(pid).await"
        );
        (session_id, pid, global_claude::is_process_alive(pid))
    });
    let outcomes = match tokio::time::timeout(CLEANUP_GRACE, futures::future::join_all(kills)).await
    {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(
                module = "pid_registry",
                grace_s = CLEANUP_GRACE.as_secs(),
                pending = kill_count,
                "startup kill window expired before all SIGTERMs confirmed"
            );
            Vec::new()
        }
    };

    let mut killed = 0u32;
    let mut failed = 0u32;
    for (session_id, pid, still_alive) in outcomes {
        if still_alive {
            failed += 1;
            tracing::warn!(
                module = "pid_registry",
                %session_id,
                %pid,
                "orphan subprocess survived SIGTERM + SIGKILL attempt"
            );
        } else {
            killed += 1;
        }
    }

    let _guard = acquire_lock()?;
    save(&PidMap::new())?;

    if failed > 0 || skipped_pid_reuse > 0 {
        tracing::warn!(
            module = "pid_registry",
            entries = before,
            killed,
            already_dead,
            failed,
            skipped_pid_reuse,
            "startup: cleared registry; some orphans survived or were skipped due to PID reuse and will continue to consume credentials until they exit on their own"
        );
    } else {
        tracing::info!(
            module = "pid_registry",
            entries = before,
            killed,
            already_dead,
            failed,
            skipped_pid_reuse,
            "startup: killed orphan subprocesses and cleared registry"
        );
    }
    Ok(())
}

/// Snapshot of the current PID map. Used by graceful shutdown to signal
/// every live subprocess before the daemon exits.
pub fn snapshot() -> Result<PidMap> {
    let _guard = acquire_lock()?;
    load()
}
