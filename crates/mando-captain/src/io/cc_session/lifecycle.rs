//! Session lifecycle: recovery, expiry cleanup, has_session, and Drop.

use std::sync::Arc;

use tracing::{info, warn};

use super::{CcSession, CcSessionManager, RecoverStats};

impl CcSessionManager {
    /// Check if a session exists and is not expired.
    pub fn has_session(&self, key: &str) -> bool {
        use time::format_description::well_known::Rfc3339;
        let sessions = self.sessions_lock();
        sessions.get(key).is_some_and(|s| {
            time::OffsetDateTime::parse(&s.last_active, &Rfc3339)
                .map_err(|e| {
                    tracing::error!(module = "cc-session", key = %key, error = %e, "unparseable last_active on persisted session; treating as expired")
                })
                .ok()
                .is_some_and(|t| {
                    let elapsed = (time::OffsetDateTime::now_utc() - t).as_seconds_f64() as u64;
                    elapsed < s.idle_ttl_s
                })
        })
    }

    /// Remove all expired sessions (idle beyond their TTL).
    ///
    /// Re-checks expiry under the sync `sessions` lock for each key
    /// immediately before removal, so a session refreshed by a concurrent
    /// `follow_up` between scan and remove cannot be deleted.
    pub fn cleanup_expired(&self) -> usize {
        use time::format_description::well_known::Rfc3339;

        fn is_expired(s: &CcSession) -> bool {
            time::OffsetDateTime::parse(&s.last_active, &Rfc3339)
                .ok()
                .is_none_or(|t| {
                    let elapsed = (time::OffsetDateTime::now_utc() - t).as_seconds_f64() as u64;
                    elapsed >= s.idle_ttl_s
                })
        }

        // Phase 1: drain expired entries from the map under a single lock.
        // Holding the lock across the iterate+remove keeps the removal
        // atomic with the expiry recheck -- no concurrent refresh can slip
        // in between the check and the remove.
        let removed: Vec<(String, CcSession)> = {
            let mut sessions = self.sessions_lock();
            let keys: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| is_expired(s))
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| {
                    // Re-check under the same lock -- a concurrent
                    // `follow_up` holding the per-key async mutex could
                    // NOT have refreshed yet (it would need the sync lock
                    // to write), so this check is authoritative.
                    match sessions.get(&k) {
                        Some(s) if is_expired(s) => sessions.remove(&k).map(|s| (k, s)),
                        _ => None,
                    }
                })
                .collect()
        };

        let count = removed.len();
        // Phase 2: best-effort file cleanup for the entries we actually
        // removed. Safe to release the sync lock for I/O: the key has
        // been removed from the map, so any concurrent caller with the
        // same key either sees "no session" (start will create a new
        // file after we finish deleting the old one -- correct) or holds
        // the per-key async mutex (their write to `sessions` and persist
        // will happen after us).
        for (key, _) in &removed {
            let path = self.session_path(key);
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!(module = "cc-session", key = %key, error = %e, "failed to remove expired session file");
                }
            }
            info!(module = "cc-session", key = %key, "expired session cleaned up");
        }

        // Also prune idle per-key mutexes that no one is holding -- their
        // only reason to exist is to serialize live callers. Safe because
        // any future caller will recreate the mutex on demand.
        {
            let mut locks = self
                .key_locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locks.retain(|_, mu| Arc::strong_count(mu) > 1);
        }

        count
    }

    /// Recover sessions from disk on restart. Returns recovered count and
    /// corrupt-file count so callers can surface both.
    pub fn recover(&self) -> RecoverStats {
        let dir = &self.state_dir;
        if !dir.is_dir() {
            return RecoverStats::default();
        }
        let mut stats = RecoverStats::default();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(e) => {
                            warn!(module = "cc-session", error = %e, "failed to read directory entry during recovery");
                            stats.corrupt += 1;
                            continue;
                        }
                    };
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let data = match std::fs::read_to_string(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!(module = "cc-session", path = %path.display(), error = %e, "failed to read session file during recovery");
                            stats.corrupt += 1;
                            continue;
                        }
                    };
                    match serde_json::from_str::<CcSession>(&data) {
                        Ok(session) => {
                            let key = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            if !key.is_empty() {
                                let mut sessions = self.sessions_lock();
                                sessions.insert(key, session);
                                stats.recovered += 1;
                            }
                        }
                        Err(e) => {
                            warn!(module = "cc-session", path = %path.display(), error = %e, "corrupt session file during recovery");
                            stats.corrupt += 1;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(module = "cc-session", dir = %dir.display(), error = %e, "failed to read session directory during recovery");
            }
        }
        if stats.recovered > 0 || stats.corrupt > 0 {
            info!(
                module = "cc-session",
                recovered = stats.recovered,
                corrupt = stats.corrupt,
                "recovered sessions from disk"
            );
        }
        stats
    }
}

impl Drop for CcSessionManager {
    fn drop(&mut self) {
        let count = self.sessions.get_mut().map(|m| m.len()).unwrap_or(0);
        if count > 0 {
            warn!(
                module = "cc-session",
                count, "dropping manager with active sessions"
            );
        }
    }
}
