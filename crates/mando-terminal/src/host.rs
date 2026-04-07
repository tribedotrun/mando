use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use crate::session::TerminalSession;
use crate::types::{CreateRequest, SessionId, SessionInfo, TerminalSize};

/// Manages all active terminal sessions. Thread-safe via interior mutability.
pub struct TerminalHost {
    sessions: std::sync::Mutex<HashMap<SessionId, Arc<TerminalSession>>>,
}

impl TerminalHost {
    pub fn new() -> Self {
        Self {
            sessions: std::sync::Mutex::new(HashMap::new()),
        }
    }

    const MAX_SESSIONS: usize = 20;

    pub fn create(&self, req: CreateRequest) -> anyhow::Result<Arc<TerminalSession>> {
        let sessions = self.sessions.lock().expect("sessions lock");
        if sessions.len() >= Self::MAX_SESSIONS {
            anyhow::bail!(
                "terminal session limit reached ({}/{})",
                sessions.len(),
                Self::MAX_SESSIONS
            );
        }
        drop(sessions);
        let id = mando_uuid::Uuid::v4().to_string();
        let size = req.size.unwrap_or_default();
        info!(
            session = id,
            project = req.project,
            agent = %req.agent,
            cwd = %req.cwd.display(),
            "spawning terminal session"
        );
        let session = TerminalSession::spawn(
            id.clone(),
            req.project,
            req.cwd,
            req.agent,
            req.resume_session_id.as_deref(),
            size,
        )?;
        let mut sessions = self.sessions.lock().expect("sessions lock");
        if sessions.len() >= Self::MAX_SESSIONS {
            let _ = session.kill();
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
            .map(|s| s.info())
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
        if let Some(ref s) = session {
            if s.is_running() {
                let _ = s.kill();
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

    pub fn shutdown(&self) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .expect("sessions lock")
            .drain()
            .collect();
        for (id, session) in sessions {
            if session.is_running() {
                if let Err(e) = session.kill() {
                    tracing::warn!(session = id, error = %e, "failed to kill session on shutdown");
                }
            }
        }
    }
}

impl Default for TerminalHost {
    fn default() -> Self {
        Self::new()
    }
}
