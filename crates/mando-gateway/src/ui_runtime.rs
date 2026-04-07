use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

const MAX_SPAWN_FAILURES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UiDesiredState {
    #[default]
    Running,
    Suppressed,
    Updating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLaunchSpec {
    pub exec_path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUiState {
    #[serde(default)]
    desired_state: UiDesiredState,
    launch_spec: Option<UiLaunchSpec>,
}

impl Default for PersistedUiState {
    fn default() -> Self {
        Self {
            desired_state: UiDesiredState::Running,
            launch_spec: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStatus {
    pub desired_state: UiDesiredState,
    pub current_pid: Option<i32>,
    pub launch_available: bool,
    pub running: bool,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub restart_count: u32,
}

const UPDATE_GRACE_PERIOD: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
struct UiRuntimeState {
    desired_state: UiDesiredState,
    launch_spec: Option<UiLaunchSpec>,
    current_pid: Option<i32>,
    last_error: Option<String>,
    consecutive_failures: u32,
    degraded: bool,
    restart_count: u32,
    /// Set when entering `Updating` so tick() can wait for the new Electron
    /// instance to self-register before falling back to a manual spawn.
    updating_since: Option<Instant>,
}

#[derive(Clone)]
pub struct UiRuntime {
    state_path: PathBuf,
    inner: Arc<Mutex<UiRuntimeState>>,
}

impl UiRuntime {
    pub fn new(state_path: PathBuf) -> Self {
        let persisted = match std::fs::read_to_string(&state_path) {
            Ok(raw) => match serde_json::from_str::<PersistedUiState>(&raw) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        module = "ui",
                        path = %state_path.display(),
                        error = %e,
                        "corrupt ui-state.json, falling back to defaults"
                    );
                    PersistedUiState::default()
                }
            },
            Err(_) => PersistedUiState::default(),
        };

        Self {
            state_path,
            inner: Arc::new(Mutex::new(UiRuntimeState {
                desired_state: persisted.desired_state,
                launch_spec: persisted.launch_spec,
                current_pid: None,
                last_error: None,
                consecutive_failures: 0,
                degraded: false,
                restart_count: 0,
                updating_since: None,
            })),
        }
    }

    pub fn start_monitor(&self, tracker: &TaskTracker, cancel: CancellationToken) {
        let runtime = self.clone();
        tracker.spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
                if let Err(err) = runtime.tick().await {
                    tracing::warn!(module = "ui", error = %err, "ui supervisor tick failed");
                }
            }
        });
    }

    pub async fn register(&self, pid: i32, launch_spec: UiLaunchSpec) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.current_pid = Some(pid);
        state.launch_spec = Some(launch_spec);
        state.desired_state = UiDesiredState::Running;
        state.updating_since = None;
        state.last_error = None;
        state.consecutive_failures = 0;
        state.degraded = false;
        self.persist_locked(&state)?;
        Ok(())
    }

    /// Set the launch spec without spawning. Used by daemon auto-spawn to
    /// register the Electron binary path from env vars.
    pub async fn set_launch_spec(&self, spec: UiLaunchSpec) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.launch_spec = Some(spec);
        self.persist_locked(&state)?;
        Ok(())
    }

    pub async fn mark_quitting(&self) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.desired_state = UiDesiredState::Suppressed;
        state.current_pid = None;
        self.persist_locked(&state)?;
        Ok(())
    }

    pub async fn mark_updating(&self) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.desired_state = UiDesiredState::Updating;
        state.current_pid = None;
        state.updating_since = Some(Instant::now());
        self.persist_locked(&state)?;
        Ok(())
    }

    pub async fn launch(&self) -> anyhow::Result<()> {
        let spec = {
            let mut state = self.inner.lock().await;
            state.desired_state = UiDesiredState::Running;
            state.consecutive_failures = 0;
            state.degraded = false;
            self.persist_locked(&state)?;
            if let Some(pid) = state.current_pid {
                if is_pid_alive(pid) {
                    return Ok(());
                }
                state.current_pid = None;
            }
            state
                .launch_spec
                .clone()
                .ok_or_else(|| anyhow!("ui launch unavailable: no registered launch spec"))?
        };
        self.spawn_from_spec(spec).await
    }

    pub async fn restart(&self) -> anyhow::Result<()> {
        let (pid, spec) = {
            let mut state = self.inner.lock().await;
            state.desired_state = UiDesiredState::Running;
            state.consecutive_failures = 0;
            state.degraded = false;
            self.persist_locked(&state)?;
            let spec = state
                .launch_spec
                .clone()
                .ok_or_else(|| anyhow!("ui restart unavailable: no registered launch spec"))?;
            (state.current_pid, spec)
        };

        if let Some(pid) = pid.filter(|pid| is_pid_alive(*pid)) {
            terminate_pid(pid)?;
            // Wait for process to exit before spawning replacement.
            for _ in 0..25 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if !is_pid_alive(pid) {
                    break;
                }
            }
            {
                let mut state = self.inner.lock().await;
                state.current_pid = None;
            }
        }

        self.spawn_from_spec(spec).await
    }

    pub async fn status(&self) -> UiStatus {
        let state = self.inner.lock().await;
        let running = state.current_pid.is_some_and(is_pid_alive);
        UiStatus {
            desired_state: state.desired_state,
            current_pid: state.current_pid,
            launch_available: state.launch_spec.is_some(),
            running,
            last_error: state.last_error.clone(),
            degraded: state.degraded,
            restart_count: state.restart_count,
        }
    }

    /// Terminate the Electron child during daemon shutdown.
    /// Skips if UI state is `Updating` (Electron already exited for an update).
    pub async fn shutdown(&self) {
        let (pid, is_updating) = {
            let state = self.inner.lock().await;
            (
                state.current_pid,
                state.desired_state == UiDesiredState::Updating,
            )
        };

        if is_updating {
            tracing::info!(module = "ui", "skipping Electron SIGTERM (state=updating)");
            return;
        }

        let Some(pid) = pid.filter(|p| is_pid_alive(*p)) else {
            return;
        };

        tracing::info!(module = "ui", pid, "sending SIGTERM to Electron child");
        if let Err(e) = terminate_pid(pid) {
            tracing::warn!(module = "ui", pid, error = %e, "failed to SIGTERM Electron");
            return;
        }

        // Wait up to 5s for graceful exit, then SIGKILL.
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if !is_pid_alive(pid) {
                tracing::info!(module = "ui", pid, "Electron exited cleanly");
                let mut state = self.inner.lock().await;
                state.current_pid = None;
                return;
            }
        }

        tracing::warn!(
            module = "ui",
            pid,
            "Electron did not exit after 5s, sending SIGKILL"
        );
        unsafe { libc::kill(pid, libc::SIGKILL) };
        let mut state = self.inner.lock().await;
        state.current_pid = None;
    }

    async fn tick(&self) -> anyhow::Result<()> {
        enum Action {
            None,
            Spawn(UiLaunchSpec),
        }

        let action = {
            let mut state = self.inner.lock().await;
            if let Some(pid) = state.current_pid {
                if is_pid_alive(pid) {
                    return Ok(());
                }
                state.current_pid = None;
            }

            if state.degraded {
                return Ok(());
            }

            match state.desired_state {
                UiDesiredState::Running => {
                    if let Some(spec) = state.launch_spec.clone() {
                        Action::Spawn(spec)
                    } else {
                        Action::None
                    }
                }
                UiDesiredState::Updating => {
                    let elapsed = state
                        .updating_since
                        .map(|t| t.elapsed())
                        .unwrap_or(Duration::ZERO);
                    if elapsed < UPDATE_GRACE_PERIOD {
                        // Wait for the new Electron to self-register via
                        // POST /api/ui/register after app.relaunch().
                        Action::None
                    } else if let Some(spec) = state.launch_spec.clone() {
                        // Grace period expired -- fall back to manual spawn.
                        state.desired_state = UiDesiredState::Running;
                        state.updating_since = None;
                        self.persist_locked(&state)?;
                        Action::Spawn(spec)
                    } else {
                        Action::None
                    }
                }
                UiDesiredState::Suppressed => Action::None,
            }
        };

        if let Action::Spawn(spec) = action {
            if let Err(e) = self.spawn_from_spec(spec).await {
                let mut state = self.inner.lock().await;
                state.consecutive_failures += 1;
                let msg = format!("ui spawn failed: {e}");
                tracing::warn!(
                    module = "ui",
                    failure_count = state.consecutive_failures,
                    "{msg}"
                );
                state.last_error = Some(msg);
                if state.consecutive_failures >= MAX_SPAWN_FAILURES {
                    state.degraded = true;
                    tracing::error!(
                        module = "ui",
                        failures = state.consecutive_failures,
                        "ui entered degraded mode -- manual launch or config change required"
                    );
                }
                return Ok(());
            }
        }

        Ok(())
    }

    async fn spawn_from_spec(&self, spec: UiLaunchSpec) -> anyhow::Result<()> {
        let mut command = Command::new(&spec.exec_path);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn ui process {}", spec.exec_path))?;
        let pid = child
            .id()
            .map(|value| value as i32)
            .ok_or_else(|| anyhow!("ui spawn returned no pid"))?;

        {
            let mut state = self.inner.lock().await;
            state.current_pid = Some(pid);
            state.last_error = None;
            state.consecutive_failures = 0;
            state.restart_count += 1;
        }

        let runtime = self.clone();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    if !status.success() {
                        runtime
                            .record_error(format!("ui exited with status {status}"))
                            .await;
                    }
                }
                Err(err) => {
                    runtime.record_error(format!("ui wait failed: {err}")).await;
                }
            }
            let mut state = runtime.inner.lock().await;
            state.current_pid = None;
        });

        Ok(())
    }

    async fn record_error(&self, message: String) {
        tracing::warn!(module = "ui", "{message}");
        let mut state = self.inner.lock().await;
        state.last_error = Some(message);
    }

    fn persist_locked(&self, state: &UiRuntimeState) -> anyhow::Result<()> {
        let persisted = PersistedUiState {
            desired_state: state.desired_state,
            launch_spec: state.launch_spec.clone(),
        };
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&self.state_path, serde_json::to_vec_pretty(&persisted)?)
            .with_context(|| format!("failed to write {}", self.state_path.display()))?;
        Ok(())
    }
}

fn terminate_pid(pid: i32) -> anyhow::Result<()> {
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(anyhow!(
            "failed to terminate ui pid {pid}: {}",
            std::io::Error::last_os_error()
        ))
    }
}

fn is_pid_alive(pid: i32) -> bool {
    let rc = unsafe { libc::kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}
