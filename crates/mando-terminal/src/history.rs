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
    #[serde(default)]
    pub name: Option<String>,
    /// Claude Code session ID captured from `~/.claude/sessions/{pid}.json`.
    /// Used for `--resume <id>` on auto-resume after daemon restart.
    #[serde(default)]
    pub cc_session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TerminalHistoryStore {
    root: PathBuf,
}

impl TerminalHistoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let root = data_dir.join("terminal-history");
        if let Err(err) = fs::create_dir_all(&root) {
            warn!(path = %root.display(), error = %err, "failed to create terminal history root");
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
                Ok(Some(meta)) => sessions.push(meta),
                Ok(None) => {}
                Err(err) => {
                    warn!(path = %path.display(), error = %err, "failed to load terminal history")
                }
            }
        }

        sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        sessions
    }

    fn load_session_dir(&self, dir: &Path) -> anyhow::Result<Option<TerminalHistoryMeta>> {
        let meta_path = dir.join("meta.json");
        if !meta_path.exists() {
            return Ok(None);
        }
        let meta: TerminalHistoryMeta = serde_json::from_slice(&fs::read(&meta_path)?)?;
        if meta.id.contains('/') || meta.id.contains('\\') || meta.id.contains("..") {
            anyhow::bail!(
                "terminal history meta contains invalid session id: {}",
                meta.id
            );
        }
        Ok(Some(meta))
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
        let path =
            std::env::temp_dir().join(format!("mando-terminal-history-{}", mando_uuid::Uuid::v4()));
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
}
