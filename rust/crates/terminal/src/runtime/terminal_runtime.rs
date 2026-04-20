use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::runtime::TerminalHost;
use crate::types::{Agent, SessionInfo, TerminalSize};

#[derive(Clone)]
pub struct TerminalRuntime {
    host: Arc<TerminalHost>,
    settings: Arc<settings::SettingsRuntime>,
    listen_port: u16,
    task_tracker: TaskTracker,
    cancellation_token: CancellationToken,
    auth_token_resolver: Arc<dyn Fn() -> String + Send + Sync>,
}

#[derive(Debug, Clone)]
pub struct CreateTerminalArgs {
    pub project: String,
    pub cwd: PathBuf,
    pub agent: Agent,
    pub resume_session_id: Option<String>,
    pub size: Option<TerminalSize>,
    pub terminal_id: Option<String>,
    pub name: Option<String>,
}

impl TerminalRuntime {
    pub fn new(
        host: Arc<TerminalHost>,
        settings: Arc<settings::SettingsRuntime>,
        listen_port: u16,
        task_tracker: TaskTracker,
        cancellation_token: CancellationToken,
        auth_token_resolver: Arc<dyn Fn() -> String + Send + Sync>,
    ) -> Self {
        Self {
            host,
            settings,
            listen_port,
            task_tracker,
            cancellation_token,
            auth_token_resolver,
        }
    }

    pub fn host(&self) -> &Arc<TerminalHost> {
        &self.host
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.host.list()
    }

    pub fn info(&self, id: &str) -> Option<SessionInfo> {
        self.host.get(id).map(|session| session.info())
    }

    pub fn session(&self, id: &str) -> Option<Arc<crate::runtime::TerminalSession>> {
        self.host.get(id)
    }

    pub fn remove(&self, id: &str) -> Option<Arc<crate::runtime::TerminalSession>> {
        self.host.remove(id)
    }

    pub fn resize(&self, id: &str, size: TerminalSize) -> anyhow::Result<()> {
        self.host.resize(id, size)
    }

    pub fn shutdown(&self) {
        self.host.shutdown();
    }

    pub fn create(
        &self,
        args: CreateTerminalArgs,
    ) -> anyhow::Result<Arc<crate::runtime::TerminalSession>> {
        let mut terminal_env = HashMap::new();
        terminal_env.insert("MANDO_PORT".to_string(), self.listen_port.to_string());
        terminal_env.insert("MANDO_AUTH_TOKEN".to_string(), (self.auth_token_resolver)());

        let cfg = self.settings.load_config();
        let args_str = match &args.agent {
            Agent::Claude => cfg.captain.claude_terminal_args.clone(),
            Agent::Codex => cfg.captain.codex_terminal_args.clone(),
        };
        let config_env = cfg.env.clone();
        drop(cfg);

        let extra_args = shell_words::split(&args_str)?;

        let req = crate::types::CreateRequest {
            project: args.project,
            cwd: args.cwd,
            agent: args.agent,
            resume_session_id: args.resume_session_id,
            size: args.size,
            config_env,
            terminal_env,
            terminal_id: args.terminal_id,
            extra_args,
            name: args.name,
        };
        self.host.create(req)
    }

    pub fn start_auto_resume(&self) {
        let host = self.host.clone();
        let settings = self.settings.clone();
        let listen_port = self.listen_port;
        let cancel = self.cancellation_token.clone();
        let auth_token_resolver = self.auth_token_resolver.clone();

        self.task_tracker.spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                _ = cancel.cancelled() => { return; }
            }

            let restorable = host.take_restorable();
            if restorable.is_empty() {
                return;
            }

            let total = restorable.len();
            tracing::info!(
                module = "terminal",
                count = total,
                "auto-resuming terminal sessions"
            );

            let mut resumed = 0usize;
            let mut skipped = 0usize;
            for old in restorable {
                if cancel.is_cancelled() {
                    tracing::info!(module = "terminal", "auto-resume cancelled");
                    return;
                }
                if old.cc_session_id.is_none() {
                    tracing::info!(
                        module = "terminal",
                        session = old.id,
                        "skipping auto-resume — no CC session ID captured"
                    );
                    skipped += 1;
                    continue;
                }
                if !old.cwd.is_dir() {
                    tracing::info!(
                        module = "terminal",
                        session = old.id,
                        cwd = %old.cwd.display(),
                        "skipping auto-resume — cwd no longer exists"
                    );
                    skipped += 1;
                    continue;
                }

                let mut terminal_env = HashMap::new();
                terminal_env.insert("MANDO_PORT".to_string(), listen_port.to_string());
                terminal_env.insert("MANDO_AUTH_TOKEN".to_string(), auth_token_resolver());

                let cfg = settings.load_config();
                let args_str = match &old.agent {
                    Agent::Claude => cfg.captain.claude_terminal_args.clone(),
                    Agent::Codex => cfg.captain.codex_terminal_args.clone(),
                };
                let config_env = cfg.env.clone();
                drop(cfg);

                let extra_args = match shell_words::split(&args_str) {
                    Ok(args) => args,
                    Err(err) => {
                        tracing::warn!(
                            module = "terminal",
                            session = old.id,
                            error = %err,
                            "failed to parse terminal args for auto-resume"
                        );
                        skipped += 1;
                        continue;
                    }
                };

                let req = crate::types::CreateRequest {
                    project: old.project.clone(),
                    cwd: old.cwd.clone(),
                    agent: old.agent.clone(),
                    resume_session_id: old.cc_session_id.clone(),
                    size: Some(old.size),
                    config_env,
                    terminal_env,
                    terminal_id: old.terminal_id.clone(),
                    extra_args,
                    name: old.name.clone(),
                };

                match host.create(req) {
                    Ok(session) => {
                        host.delete_restored_history(&old.id);
                        resumed += 1;
                        tracing::info!(
                            module = "terminal",
                            old_id = old.id,
                            new_id = session.info().id,
                            project = old.project,
                            "auto-resumed terminal session"
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            module = "terminal",
                            session = old.id,
                            error = %err,
                            "failed to auto-resume terminal session"
                        );
                    }
                }
            }

            tracing::info!(
                module = "terminal",
                total,
                resumed,
                skipped,
                failed = total - resumed - skipped,
                "terminal auto-resume complete"
            );
        });
    }
}
