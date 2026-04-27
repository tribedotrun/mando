use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::types::{Agent, SessionState, TerminalSize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalHistoryMeta {
    pub id: String,
    pub project: String,
    pub cwd: PathBuf,
    pub agent: Agent,
    pub terminal_id: Option<String>,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<u32>,
    pub size: TerminalSize,
    pub state: SessionState,
    pub name: Option<String>,
    /// Claude Code session ID captured from `~/.claude/sessions/{pid}.json`.
    /// Used for `--resume <id>` on auto-resume after daemon restart.
    pub cc_session_id: Option<String>,
    /// Workbench owning this session. Stamped at create time so a daemon
    /// restart re-binds the auto-resumed session to the same workbench
    /// instead of re-deriving from `cwd` (which leaks across workbenches
    /// when `cwd` happens to equal the project root).
    pub workbench_id: i64,
}

#[derive(Debug, Clone)]
pub struct TerminalHistoryStore {
    root: PathBuf,
}

/// Outcome of loading a single `meta.json` from disk. Splitting eviction
/// (legacy schema) from skip-and-keep (transient failure) at the
/// type level so a flaky read can't permanently delete a healthy row.
enum LoadOutcome {
    Loaded(Box<TerminalHistoryMeta>),
    Skipped,
    LegacyMissingWorkbenchId,
    Failed(anyhow::Error),
}

impl TerminalHistoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let root = data_dir.join("terminal-history");
        if let Err(err) = fs::create_dir_all(&root) {
            warn!(module = "terminal", path = %root.display(), error = %err, "failed to create terminal history root");
        }
        Self { root }
    }

    pub fn create_session(&self, meta: &TerminalHistoryMeta) -> anyhow::Result<()> {
        fs::create_dir_all(self.session_dir(&meta.id))?;
        self.write_meta(meta)
    }

    pub fn finish_session(
        &self,
        id: &str,
        exit_code: Option<u32>,
        ended_at: String,
    ) -> anyhow::Result<()> {
        let mut meta = self
            .read_meta(id)?
            .ok_or_else(|| anyhow::anyhow!("missing terminal history meta for session {id}"))?;
        meta.exit_code = exit_code;
        meta.ended_at = Some(ended_at);
        meta.state = SessionState::Exited;
        self.write_meta(&meta)
    }

    pub fn set_cc_session_id(&self, id: &str, cc_session_id: String) -> anyhow::Result<()> {
        let mut meta = self
            .read_meta(id)?
            .ok_or_else(|| anyhow::anyhow!("missing terminal history meta for session {id}"))?;
        meta.cc_session_id = Some(cc_session_id);
        self.write_meta(&meta)
    }

    pub fn update_size(&self, id: &str, size: TerminalSize) -> anyhow::Result<()> {
        let mut meta = self
            .read_meta(id)?
            .ok_or_else(|| anyhow::anyhow!("missing terminal history meta for session {id}"))?;
        meta.size = size;
        self.write_meta(&meta)
    }

    pub fn delete_session(&self, id: &str) -> anyhow::Result<()> {
        let dir = self.session_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn load_sessions(&self) -> Vec<TerminalHistoryMeta> {
        let mut sessions = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return sessions,
            Err(err) => {
                warn!(
                    module = "terminal",
                    path = %self.root.display(),
                    error = %err,
                    "failed to read terminal history root"
                );
                return sessions;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match self.load_session_dir(&path) {
                LoadOutcome::Loaded(meta) => sessions.push(*meta),
                LoadOutcome::Skipped => {}
                LoadOutcome::LegacyMissingWorkbenchId => {
                    // Legacy meta.json that predates `workbench_id`: drop
                    // the row. Backfilling via `find_by_worktree(cwd)`
                    // would re-introduce the cwd-based heuristic this
                    // fix is removing, and auto-resume only reaches the
                    // renderer through a workbench-scoped tab bar — a
                    // session whose workbench can't be re-derived has no
                    // surface to appear on. The user can re-spawn from
                    // the workbench page if needed.
                    warn!(module = "terminal", path = %path.display(), "evicting terminal history — pre-workbench_id meta");
                    if let Err(rm_err) = fs::remove_dir_all(&path) {
                        warn!(module = "terminal", path = %path.display(), error = %rm_err, "failed to remove evicted terminal history dir");
                    }
                }
                LoadOutcome::Failed(err) => {
                    // Transient I/O errors (permissions, brief disk
                    // hiccup) and unexpected JSON corruption MUST NOT
                    // delete the row — that would permanently destroy
                    // valid session history on a flaky read. Log and
                    // skip; next startup retries.
                    warn!(module = "terminal", path = %path.display(), error = %err, "failed to load terminal history meta — keeping on disk for retry");
                }
            }
        }

        sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        sessions
    }

    fn load_session_dir(&self, dir: &Path) -> LoadOutcome {
        let meta_path = dir.join("meta.json");
        if !meta_path.exists() {
            return LoadOutcome::Skipped;
        }
        let bytes = match fs::read(&meta_path) {
            Ok(b) => b,
            Err(e) => return LoadOutcome::Failed(e.into()),
        };
        // Two-phase decode: first parse to a `Value` so the legacy
        // detector can target the missing-`workbench_id` shape exactly,
        // without conflating it with unrelated parse errors. Anything
        // that already deserializes into a typed meta is healthy.
        let raw: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => return LoadOutcome::Failed(e.into()),
        };
        let object = match raw.as_object() {
            Some(o) => o,
            None => {
                return LoadOutcome::Failed(anyhow::anyhow!(
                    "terminal history meta is not a JSON object"
                ))
            }
        };
        if !object.contains_key("workbench_id") {
            return LoadOutcome::LegacyMissingWorkbenchId;
        }
        let meta: TerminalHistoryMeta = match serde_json::from_value(raw) {
            Ok(m) => m,
            Err(e) => return LoadOutcome::Failed(e.into()),
        };
        if meta.id.contains('/') || meta.id.contains('\\') || meta.id.contains("..") {
            return LoadOutcome::Failed(anyhow::anyhow!(
                "terminal history meta contains invalid session id: {}",
                meta.id
            ));
        }
        LoadOutcome::Loaded(Box::new(meta))
    }

    fn read_meta(&self, id: &str) -> anyhow::Result<Option<TerminalHistoryMeta>> {
        let path = self.meta_path(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }

    fn write_meta(&self, meta: &TerminalHistoryMeta) -> anyhow::Result<()> {
        let path = self.meta_path(&meta.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(meta)?;
        fs::write(path, data)?;
        Ok(())
    }

    fn session_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.session_dir(id).join("meta.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mando-terminal-history-{}",
            global_infra::uuid::Uuid::v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn meta(id: &str) -> TerminalHistoryMeta {
        TerminalHistoryMeta {
            id: id.to_string(),
            project: "mando".into(),
            cwd: PathBuf::from("/tmp/project"),
            agent: Agent::Claude,
            terminal_id: Some("wb:1".into()),
            created_at: "2026-04-08T00:00:00Z".into(),
            ended_at: None,
            exit_code: None,
            size: TerminalSize { rows: 24, cols: 80 },
            state: SessionState::Live,
            name: None,
            cc_session_id: None,
            workbench_id: 42,
        }
    }

    #[test]
    fn persists_and_loads_session_meta() {
        let data_dir = temp_data_dir();
        let store = TerminalHistoryStore::new(data_dir.clone());
        let meta = meta("session-a");
        store.create_session(&meta).unwrap();
        store
            .finish_session(&meta.id, Some(0), "2026-04-08T00:01:00Z".into())
            .unwrap();

        let sessions = store.load_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-a");
        assert_eq!(sessions[0].exit_code, Some(0));

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn deletes_session_history() {
        let data_dir = temp_data_dir();
        let store = TerminalHistoryStore::new(data_dir.clone());
        let meta = meta("session-c");
        store.create_session(&meta).unwrap();
        store.delete_session(&meta.id).unwrap();

        assert!(store.load_sessions().is_empty());
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn round_trips_workbench_id() {
        let data_dir = temp_data_dir();
        let store = TerminalHistoryStore::new(data_dir.clone());
        let mut m = meta("with-wb");
        m.workbench_id = 7;
        store.create_session(&m).unwrap();

        let sessions = store.load_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].workbench_id, 7);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn evicts_legacy_meta_without_workbench_id() {
        let data_dir = temp_data_dir();
        let dir = data_dir.join("terminal-history/legacy-row");
        fs::create_dir_all(&dir).unwrap();
        // meta.json shape predating workbench_id; load must drop the row
        // (and the directory) instead of returning it without an id.
        fs::write(
            dir.join("meta.json"),
            serde_json::json!({
                "id": "legacy-row",
                "project": "mando",
                "cwd": data_dir,
                "agent": "claude",
                "terminal_id": null,
                "created_at": "2026-04-08T00:00:00Z",
                "ended_at": null,
                "exit_code": null,
                "size": { "rows": 24, "cols": 80 },
                "state": "live",
                "name": null,
                "cc_session_id": null
            })
            .to_string(),
        )
        .unwrap();

        let store = TerminalHistoryStore::new(data_dir.clone());
        assert!(store.load_sessions().is_empty());
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn keeps_corrupt_meta_for_retry_without_evicting() {
        // A flaky/corrupt meta.json that is NOT the missing-workbench_id
        // shape must be left on disk so a future load retry can pick it
        // up. Permanently deleting on transient failures was rejected
        // by review (#1012, cursor + greptile).
        let data_dir = temp_data_dir();
        let dir = data_dir.join("terminal-history/corrupt-row");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("meta.json"), b"not-json").unwrap();

        let store = TerminalHistoryStore::new(data_dir.clone());
        assert!(store.load_sessions().is_empty());
        assert!(dir.exists(), "corrupt meta must NOT be evicted");
        let _ = fs::remove_dir_all(data_dir);
    }
}
