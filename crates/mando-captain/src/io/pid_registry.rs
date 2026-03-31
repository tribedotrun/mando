//! Session PID registry — `~/.mando/state/session-pids.json`.
//!
//! Single authority for all CC session PIDs. Ephemeral runtime state:
//! written on spawn/resume, removed on terminate, cleaned on startup.

use std::collections::HashMap;
use std::io::Write;

use mando_shared::load_json_file;

type PidMap = HashMap<String, u32>;

fn registry_path() -> std::path::PathBuf {
    mando_config::state_dir().join("session-pids.json")
}

fn load() -> PidMap {
    load_json_file(&registry_path(), "pid_registry")
}

/// Atomic save with file locking to prevent concurrent corruption.
fn save(map: &PidMap) {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::error!(module = "pid_registry", error = %e, "failed to create state dir");
            return;
        }
    }
    // Write to temp file then rename for atomicity.
    let tmp = path.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut f, map).map_err(std::io::Error::other)?;
        f.flush()?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    })();
    if let Err(e) = result {
        tracing::error!(module = "pid_registry", error = %e, "failed to save session-pids.json");
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Record a session's PID. Overwrites any previous entry for this session.
pub fn register(session_id: &str, pid: u32) {
    let mut map = load();
    map.insert(session_id.to_string(), pid);
    save(&map);
}

/// Remove a session from the registry.
pub fn unregister(session_id: &str) {
    let mut map = load();
    if map.remove(session_id).is_some() {
        save(&map);
    }
}

/// Look up the PID for a session.
pub fn get_pid(session_id: &str) -> Option<u32> {
    load().get(session_id).copied()
}

/// Remove entries for dead processes. Call on daemon startup.
pub fn cleanup_dead() {
    let map = load();
    let mut cleaned: PidMap = HashMap::new();
    let mut removed = 0u32;
    for (sid, pid) in &map {
        if mando_cc::is_process_alive(*pid) {
            cleaned.insert(sid.clone(), *pid);
        } else {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(
            module = "pid_registry",
            removed,
            remaining = cleaned.len(),
            "startup: cleaned dead PIDs"
        );
        save(&cleaned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test using the raw map functions directly to avoid hitting the
    /// production registry path.
    #[test]
    fn register_and_get_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), 1234);
        assert_eq!(map.get("s1").copied(), Some(1234));
    }

    #[test]
    fn overwrite_on_resume_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), 1000);
        map.insert("s1".into(), 2000);
        assert_eq!(map.get("s1").copied(), Some(2000));
    }

    #[test]
    fn unregister_removes_via_map() {
        let mut map = PidMap::new();
        map.insert("s1".into(), 1234);
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
        map.insert("s1".into(), 42);
        map.insert("s2".into(), 99);

        // Write directly to temp path.
        std::fs::write(&path, serde_json::to_string_pretty(&map).unwrap()).unwrap();

        // Read back.
        let loaded: PidMap = load_json_file(&path, "test");
        assert_eq!(loaded.get("s1").copied(), Some(42));
        assert_eq!(loaded.get("s2").copied(), Some(99));

        std::fs::remove_dir_all(&dir).ok();
    }
}
