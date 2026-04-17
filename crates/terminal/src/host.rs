use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use crate::env::ShellEnvResolver;
use crate::history::TerminalHistoryStore;
use crate::session::TerminalSession;
use crate::types::{CreateRequest, SessionId, SessionInfo, SessionState, TerminalSize};

/// Manages all active and restorable terminal sessions.
pub struct TerminalHost {
    sessions: std::sync::Mutex<HashMap<SessionId, Arc<TerminalSession>>>,
    history: Arc<TerminalHistoryStore>,
    env: Arc<ShellEnvResolver>,
}

impl TerminalHost {
    const MAX_SESSIONS: usize = 20;

    pub fn new(data_dir: PathBuf) -> Self {
        let history = Arc::new(TerminalHistoryStore::new(data_dir));
        let env = Arc::new(ShellEnvResolver::new());
        let sessions = history
            .load_sessions()
            .into_iter()
            .map(|meta| {
                let id = meta.id.clone();
                let session = TerminalSession::restored(meta, history.clone());
                (id, session)
            })
            .collect();
        Self {
            sessions: std::sync::Mutex::new(sessions),
            history,
            env,
        }
    }

    pub fn create(&self, req: CreateRequest) -> anyhow::Result<Arc<TerminalSession>> {
        {
            let mut sessions = self.sessions.lock().expect("sessions lock");
            if sessions.len() >= Self::MAX_SESSIONS {
                // Evict exited sessions to make room before giving up.
                Self::evict_exited(&mut sessions, &self.history);
            }
            if sessions.len() >= Self::MAX_SESSIONS {
                anyhow::bail!(
                    "terminal session limit reached ({}/{})",
                    sessions.len(),
                    Self::MAX_SESSIONS
                );
            }
        }

        let id = global_infra::uuid::Uuid::v4().to_string();
        info!(
            session = id,
            project = req.project,
            agent = %req.agent,
            cwd = %req.cwd.display(),
            terminal_id = ?req.terminal_id,
            "spawning terminal session"
        );

        let session =
            TerminalSession::spawn(id.clone(), req, self.history.clone(), self.env.clone())?;
        let mut sessions = self.sessions.lock().expect("sessions lock");
        if sessions.len() >= Self::MAX_SESSIONS {
            Self::evict_exited(&mut sessions, &self.history);
        }
        if sessions.len() >= Self::MAX_SESSIONS {
            let _ = session.kill();
            let _ = session.delete_history();
            anyhow::bail!(
                "terminal session limit reached ({}/{})",
                sessions.len(),
                Self::MAX_SESSIONS
            );
        }
        sessions.insert(id, session.clone());
        Ok(session)
    }

    pub fn get(&self, id: &str) -> Option<Arc<TerminalSession>> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .values()
            .map(|session| session.info())
            .collect()
    }

    pub fn kill(&self, id: &str) -> anyhow::Result<()> {
        let session = self
            .sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.kill()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<TerminalSession>> {
        let session = self.sessions.lock().expect("sessions lock").remove(id);
        if let Some(ref session) = session {
            if session.is_running() {
                let _ = session.kill();
            }
            if let Err(err) = session.delete_history() {
                warn!(session = id, error = %err, "failed to delete terminal history");
            }
        }
        session
    }

    pub fn resize(&self, id: &str, size: TerminalSize) -> anyhow::Result<()> {
        let session = self
            .sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.resize(size)
    }

    /// Remove and return all sessions that were alive when the daemon last
    /// exited (state == Restored). The caller can re-spawn them with
    /// `--resume` to continue where they left off. History is NOT deleted
    /// here -- the caller should call [`delete_restored_history`] after
    /// successfully creating the replacement session.
    pub fn take_restorable(&self) -> Vec<SessionInfo> {
        let mut sessions = self.sessions.lock().expect("sessions lock");
        let restorable_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.state() == SessionState::Restored)
            .map(|(id, _)| id.clone())
            .collect();

        restorable_ids
            .into_iter()
            .filter_map(|id| {
                let session = sessions.remove(&id)?;
                Some(session.info())
            })
            .collect()
    }

    /// Delete on-disk history for a restored session after its replacement
    /// has been successfully spawned.
    pub fn delete_restored_history(&self, id: &str) {
        if let Err(err) = self.history.delete_session(id) {
            warn!(session = id, error = %err, "failed to delete restored session history");
        }
    }

    pub fn shutdown(&self) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .expect("sessions lock")
            .drain()
            .collect();
        for (id, session) in sessions {
            if session.is_running() {
                if let Err(err) = session.kill() {
                    warn!(session = id, error = %err, "failed to kill session on shutdown");
                }
            }
        }
    }

    /// Remove all exited (not running, not restorable) sessions from the map
    /// and clean up their on-disk history. Called when the session limit is
    /// hit so dead sessions don't block new ones.
    fn evict_exited(
        sessions: &mut HashMap<SessionId, Arc<TerminalSession>>,
        history: &TerminalHistoryStore,
    ) {
        let dead: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| !s.is_running() && s.state() != SessionState::Restored)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &dead {
            sessions.remove(id);
            if let Err(err) = history.delete_session(id) {
                warn!(session = id, error = %err, "failed to delete evicted session history");
            }
        }
        if !dead.is_empty() {
            info!(count = dead.len(), "evicted exited terminal sessions");
        }
    }
}
