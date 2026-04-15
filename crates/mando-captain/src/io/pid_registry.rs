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

use anyhow::{Context, Result};
use mando_shared::load_json_file;
use mando_types::Pid;

/// Registry entry: the subprocess PID and a fingerprint captured at
/// registration time. The fingerprint is the output of
/// `ps -p <pid> -o lstart=` which is stable for the lifetime of the
/// process and differs across PID reuse.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PidEntry {
    pub pid: Pid,
    /// Process start-time fingerprint. Empty if `ps` was unavailable at
    /// registration, or if the entry was read from a legacy bare-PID
    /// registry file. An empty fingerprint fails the identity check and
    /// causes cleanup to skip the kill.
    pub started_at: String,
}

/// Accept both on-disk shapes so a daemon upgrade can read the previous
/// registry format (`{"sid": 1234}`) alongside the new format
/// (`{"sid": {"pid": 1234, "started_at": "..."}}`). Entries read in the
/// bare-PID shape come back with an empty fingerprint, which flows
/// through `cleanup_on_startup` as "cannot positively identify" and
/// triggers safe-skip rather than a potentially wrong kill.
impl<'de> serde::Deserialize<'de> for PidEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FullEntry {
            pid: Pid,
            #[serde(default)]
            started_at: String,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Full(FullEntry),
            Legacy(Pid),
        }

        Ok(match Raw::deserialize(deserializer)? {
            Raw::Full(f) => PidEntry {
                pid: f.pid,
                started_at: f.started_at,
            },
            Raw::Legacy(pid) => PidEntry {
                pid,
                started_at: String::new(),
            },
        })
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
///
/// Captures the process start-time fingerprint at registration so a
/// later cleanup pass can detect PID reuse. Fingerprint capture runs
/// outside the registry lock so we don't block other callers while
/// `ps` executes.
pub fn register(session_id: &str, pid: Pid) -> Result<()> {
    let started_at = capture_start_fingerprint(pid);
    let entry = PidEntry { pid, started_at };
    let _guard = acquire_lock()?;
    let mut map = load()?;
    map.insert(session_id.to_string(), entry);
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
/// the PID for an unrelated process). Legacy entries with an empty
/// fingerprint skip verification and are trusted if alive.
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

    if entry.pid.as_u32() == 0 || !mando_cc::is_process_alive(entry.pid) {
        return None;
    }

    // Legacy entry (empty fingerprint) -- skip verification.
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
        if entry.pid.as_u32() == 0 || !mando_cc::is_process_alive(entry.pid) {
            already_dead += 1;
            continue;
        }
        // PID reuse check: an empty stored fingerprint (registered from
        // an older daemon, or ps unavailable at register time) fails
        // closed so we never kill a PID we can't positively identify.
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
        let _ = mando_cc::kill_process(pid).await;
        (session_id, pid, mando_cc::is_process_alive(pid))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pid: u32) -> PidEntry {
        PidEntry {
            pid: Pid::new(pid),
            started_at: format!("stub-{pid}"),
        }
    }

    /// Test using the raw map functions directly to avoid hitting the
    /// production registry path.
    #[test]
    fn register_and_get_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), entry(1234));
        assert_eq!(map.get("s1").map(|e| e.pid), Some(Pid::new(1234)));
    }

    #[test]
    fn overwrite_on_resume_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), entry(1000));
        map.insert("s1".into(), entry(2000));
        assert_eq!(map.get("s1").map(|e| e.pid), Some(Pid::new(2000)));
    }

    #[test]
    fn unregister_removes_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), entry(1234));
        map.remove("s1");
        assert!(!map.contains_key("s1"));
    }

    #[test]
    fn get_missing_returns_none_via_map() {
        let map = PidMap::new();
        assert!(!map.contains_key("nonexistent"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("mando-pid-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("session-pids.json");

        let mut map = PidMap::new();
        map.insert("s1".into(), entry(42));
        map.insert("s2".into(), entry(99));

        // Write directly to temp path.
        std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap()).unwrap();

        // Read back.
        let loaded: PidMap = load_json_file(&path, "test").unwrap_or_default();
        assert_eq!(loaded.get("s1").map(|e| e.pid), Some(Pid::new(42)));
        assert_eq!(loaded.get("s2").map(|e| e.pid), Some(Pid::new(99)));
        assert_eq!(
            loaded.get("s1").map(|e| e.started_at.as_str()),
            Some("stub-42")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_bare_pid_format_deserializes_with_empty_fingerprint() {
        // A registry file written by an older daemon version stores bare
        // PID numbers: {"sid": 1234}. The upgrade path must read those
        // without erroring so cleanup_on_startup runs instead of crashing
        // the daemon on boot. Legacy entries come back with an empty
        // fingerprint, which fails the identity check in
        // cleanup_on_startup and routes them to safe-skip.
        let legacy = r#"{"old-sess": 4242, "old-sess-2": 8080}"#;
        let loaded: PidMap = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded.get("old-sess").map(|e| e.pid), Some(Pid::new(4242)));
        assert_eq!(
            loaded.get("old-sess").map(|e| e.started_at.as_str()),
            Some("")
        );
        assert_eq!(
            loaded.get("old-sess-2").map(|e| e.pid),
            Some(Pid::new(8080))
        );
    }

    #[test]
    fn mixed_legacy_and_new_entries_deserialize() {
        // Defensive: during an upgrade window a registry could in
        // principle contain entries written by different daemon versions
        // (if two daemons raced for the lock: unlikely but possible).
        // Both shapes must round-trip into the same map.
        let mixed = r#"{
            "legacy": 111,
            "modern": {"pid": 222, "started_at": "Thu Apr 14 00:00:00 2026"}
        }"#;
        let loaded: PidMap = serde_json::from_str(mixed).unwrap();
        assert_eq!(loaded.get("legacy").map(|e| e.pid), Some(Pid::new(111)));
        assert_eq!(
            loaded.get("legacy").map(|e| e.started_at.as_str()),
            Some("")
        );
        assert_eq!(loaded.get("modern").map(|e| e.pid), Some(Pid::new(222)));
        assert_eq!(
            loaded.get("modern").map(|e| e.started_at.as_str()),
            Some("Thu Apr 14 00:00:00 2026")
        );
    }

    /// Tests below mutate the process-wide `MANDO_DATA_DIR` env var so
    /// `registry_path()` resolves under a scratch directory. Share a mutex
    /// so parallel test threads don't step on each other.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn isolate_data_dir() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mando-pid-startup-{}", mando_uuid::Uuid::v4()));
        std::fs::create_dir_all(dir.join("state")).unwrap();
        std::env::set_var("MANDO_DATA_DIR", &dir);
        dir
    }

    #[tokio::test]
    async fn cleanup_on_startup_clears_dead_entries() {
        let _lock = ENV_LOCK.lock().await;
        let _dir = isolate_data_dir();
        // Register a PID that definitely does not exist (>> /proc/sys/kernel/pid_max
        // on every OS we target). The process-alive probe is just `kill(pid, 0)`
        // which returns ESRCH for unused PIDs.
        register("dead-1", Pid::new(999_999_999)).unwrap();
        register("dead-2", Pid::new(999_999_998)).unwrap();

        cleanup_on_startup().await.unwrap();

        let map = load().unwrap();
        assert!(map.is_empty(), "registry must be empty after cleanup");
    }

    #[tokio::test]
    async fn cleanup_on_startup_kills_live_entries() {
        let _lock = ENV_LOCK.lock().await;
        let _dir = isolate_data_dir();

        // Spawn `sleep 30` in its own process group so kill_process (which
        // signals the whole group via `-pid`) actually lands.
        let mut child = {
            let mut cmd = tokio::process::Command::new("sleep");
            cmd.arg("30");
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            cmd.kill_on_drop(true)
                .spawn()
                .expect("spawn sleep 30 for pid cleanup test")
        };
        let pid_u32 = child.id().expect("child pid");
        let pid = Pid::new(pid_u32);
        register("live-1", pid).unwrap();
        assert!(mando_cc::is_process_alive(pid));

        cleanup_on_startup().await.unwrap();

        let map = load().unwrap();
        assert!(map.is_empty(), "registry must be empty after cleanup");
        // Reap the zombie so that `kill(pid, 0)` no longer reports it alive.
        // Bounded wait: if cleanup didn't signal the process, wait() would
        // hang for the full `sleep 30`, which is caught by the outer tokio
        // timeout guard.
        let waited = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await;
        let status = waited
            .expect("child did not exit within 10s; cleanup failed to signal subprocess")
            .expect("wait on child failed");
        assert!(
            status.code().is_none() || status.code() == Some(0),
            "unexpected exit status {status:?}"
        );
    }

    #[tokio::test]
    async fn cleanup_skips_kill_when_fingerprint_mismatches() {
        // Simulates PID reuse: the registry stores a fingerprint that
        // doesn't match the real process. cleanup must refuse to signal
        // a process it cannot positively identify as its own.
        let _lock = ENV_LOCK.lock().await;
        let _dir = isolate_data_dir();

        let mut child = {
            let mut cmd = tokio::process::Command::new("sleep");
            cmd.arg("30");
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            cmd.kill_on_drop(true)
                .spawn()
                .expect("spawn sleep 30 for pid reuse test")
        };
        let pid_u32 = child.id().expect("child pid");
        let pid = Pid::new(pid_u32);

        // Write the registry directly with a fingerprint that cannot
        // match the live process (`ps -o lstart=` never emits this).
        {
            let _guard = acquire_lock().unwrap();
            let mut map = PidMap::new();
            map.insert(
                "reused".into(),
                PidEntry {
                    pid,
                    started_at: "not-a-real-ps-fingerprint".into(),
                },
            );
            save(&map).unwrap();
        }
        assert!(mando_cc::is_process_alive(pid));

        cleanup_on_startup().await.unwrap();

        // Registry is cleared regardless of skip decisions.
        let map = load().unwrap();
        assert!(map.is_empty(), "registry must be empty after cleanup");
        // Critical: the subprocess must still be alive because cleanup
        // refused to signal a fingerprint-mismatched PID.
        assert!(
            mando_cc::is_process_alive(pid),
            "cleanup killed a process whose fingerprint did not match"
        );

        // Kill manually so the test doesn't leave a zombie.
        unsafe { libc::kill(-(pid.as_i32()), libc::SIGKILL) };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
    }
}
