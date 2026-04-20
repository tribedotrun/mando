use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::{AUTO_REGISTER_WAIT, MAX_SPAWN_FAILURES};
use crate::io::{process, state_fs};
use crate::service::{autolaunch, lifecycle};
use crate::types::{UiAutoLaunchOptions, UiDesiredState, UiLaunchSpec, UiStatus};

#[derive(Debug, Default)]
struct UiRuntimeState {
    desired_state: UiDesiredState,
    launch_spec: Option<UiLaunchSpec>,
    current_pid: Option<i32>,
    last_error: Option<String>,
    consecutive_failures: u32,
    degraded: bool,
    restart_count: u32,
    updating_since: Option<Instant>,
}

#[derive(Clone)]
pub struct UiRuntime {
    state_path: PathBuf,
    inner: Arc<Mutex<UiRuntimeState>>,
}

impl UiRuntime {
    pub fn new(state_path: PathBuf) -> Self {
        let persisted = state_fs::load_state(&state_path);
        let launch_spec = persisted
            .launch_spec
            .map(Into::<UiLaunchSpec>::into)
            .filter(|spec: &UiLaunchSpec| {
            let has_auth_token = spec.env.contains_key("MANDO_AUTH_TOKEN");
            if !has_auth_token {
                tracing::info!(
                    module = "ui",
                    "discarding persisted UI launch spec without auth token; waiting for fresh daemon auto-launch spec"
                );
            }
            has_auth_token
        });
        Self {
            state_path,
            inner: Arc::new(Mutex::new(UiRuntimeState {
                desired_state: persisted.desired_state,
                launch_spec,
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

    pub fn schedule_daemon_autolaunch(
        &self,
        tracker: &TaskTracker,
        cancel: CancellationToken,
        options: UiAutoLaunchOptions,
    ) {
        let runtime = self.clone();
        tracker.spawn(async move {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(AUTO_REGISTER_WAIT) => {}
            }

            let status = runtime.status().await;
            if status.running {
                tracing::info!(module = "transport-ui-runtime-ui_runtime", "Electron already registered, skipping auto-spawn");
                return;
            }
            // Env-var-driven autolaunch is only invoked by dev/prod-local scripts
            // that explicitly want Electron spawned — override any persisted
            // Suppressed flag left over from a previous app quit.
            if status.desired_state == UiDesiredState::Suppressed {
                tracing::info!(
                    module = "ui",
                    "overriding persisted Suppressed state for env-var-driven auto-spawn"
                );
            }
            if cancel.is_cancelled() {
                return;
            }

            let spec = autolaunch::build_launch_spec(&options);
            if let Err(err) = runtime.set_launch_spec(spec).await {
                tracing::warn!(module = "ui", error = %err, "failed to set launch spec for auto-spawn");
                return;
            }
            if cancel.is_cancelled() {
                return;
            }
            if let Err(err) = runtime.launch().await {
                tracing::warn!(module = "ui", error = %err, "failed to auto-spawn Electron");
            }
        });
    }

    #[tracing::instrument(skip_all)]
    pub async fn register(&self, pid: i32, launch_spec: UiLaunchSpec) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.current_pid = Some(pid);
        state.launch_spec = Some(launch_spec);
        state.desired_state = UiDesiredState::Running;
        state.updating_since = None;
        state.last_error = None;
        state.consecutive_failures = 0;
        state.degraded = false;
        self.persist_locked(&state)
    }

    #[tracing::instrument(skip_all)]
    pub async fn set_launch_spec(&self, spec: UiLaunchSpec) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.launch_spec = Some(spec);
        self.persist_locked(&state)
    }

    #[tracing::instrument(skip_all)]
    pub async fn mark_quitting(&self) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.desired_state = UiDesiredState::Suppressed;
        state.current_pid = None;
        self.persist_locked(&state)
    }

    #[tracing::instrument(skip_all)]
    pub async fn mark_updating(&self) -> anyhow::Result<()> {
        let mut state = self.inner.lock().await;
        state.desired_state = UiDesiredState::Updating;
        state.current_pid = None;
        state.updating_since = Some(Instant::now());
        self.persist_locked(&state)
    }

    #[tracing::instrument(skip_all)]
    pub async fn launch(&self) -> anyhow::Result<()> {
        let spec = {
            let mut state = self.inner.lock().await;
            state.desired_state = UiDesiredState::Running;
            state.consecutive_failures = 0;
            state.degraded = false;
            self.persist_locked(&state)?;
            if let Some(pid) = state.current_pid {
                if process::is_pid_alive(pid) {
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

    #[tracing::instrument(skip_all)]
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

        if let Some(pid) = pid.filter(|pid| process::is_pid_alive(*pid)) {
            process::terminate_pid(pid)?;
            let mut exited = false;
            for _ in 0..25 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if !process::is_pid_alive(pid) {
                    exited = true;
                    break;
                }
            }
            if !exited {
                process::force_kill_pid(pid);
                for _ in 0..25 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if !process::is_pid_alive(pid) {
                        break;
                    }
                }
            }
            let mut state = self.inner.lock().await;
            state.current_pid = None;
        }

        self.spawn_from_spec(spec).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn status(&self) -> UiStatus {
        let state = self.inner.lock().await;
        lifecycle::ui_status(
            state.desired_state,
            state.current_pid,
            state.launch_spec.is_some(),
            state.current_pid.is_some_and(process::is_pid_alive),
            state.last_error.clone(),
            state.degraded,
            state.restart_count,
        )
    }

    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&self) {
        let (pid, desired_state) = {
            let state = self.inner.lock().await;
            (state.current_pid, state.desired_state)
        };

        if lifecycle::should_skip_shutdown(desired_state) {
            tracing::info!(module = "ui", "skipping Electron SIGTERM (state=updating)");
            return;
        }

        let Some(pid) = pid.filter(|pid| process::is_pid_alive(*pid)) else {
            return;
        };

        tracing::info!(module = "ui", pid, "sending SIGTERM to Electron child");
        if let Err(err) = process::terminate_pid(pid) {
            tracing::warn!(module = "ui", pid, error = %err, "failed to SIGTERM Electron");
            return;
        }

        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if !process::is_pid_alive(pid) {
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
        process::force_kill_pid(pid);
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
                if process::is_pid_alive(pid) {
                    return Ok(());
                }
                state.current_pid = None;
            }

            if state.degraded {
                return Ok(());
            }

            match state.desired_state {
                UiDesiredState::Running => state.launch_spec.clone().map(Action::Spawn),
                UiDesiredState::Updating => {
                    if !lifecycle::update_grace_expired(state.updating_since, Instant::now()) {
                        Some(Action::None)
                    } else if let Some(spec) = state.launch_spec.clone() {
                        state.desired_state = UiDesiredState::Running;
                        state.updating_since = None;
                        self.persist_locked(&state)?;
                        Some(Action::Spawn(spec))
                    } else {
                        Some(Action::None)
                    }
                }
                UiDesiredState::Suppressed => Some(Action::None),
            }
        }
        .unwrap_or(Action::None);

        if let Action::Spawn(spec) = action {
            if let Err(err) = self.spawn_from_spec(spec).await {
                let mut state = self.inner.lock().await;
                state.consecutive_failures += 1;
                let message = format!("ui spawn failed: {err}");
                tracing::warn!(
                    module = "ui",
                    failure_count = state.consecutive_failures,
                    "{message}"
                );
                state.last_error = Some(message);
                if state.consecutive_failures >= MAX_SPAWN_FAILURES {
                    state.degraded = true;
                    tracing::error!(
                        module = "ui",
                        failures = state.consecutive_failures,
                        "ui entered degraded mode -- manual launch or config change required"
                    );
                }
            }
        }

        Ok(())
    }

    async fn spawn_from_spec(&self, spec: UiLaunchSpec) -> anyhow::Result<()> {
        let (pid, mut child) = process::spawn_process(&spec)?;

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
                Ok(status) if !status.success() => {
                    runtime
                        .record_error(format!("ui exited with status {status}"))
                        .await;
                }
                Ok(_) => {}
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
        state_fs::persist_state(
            &self.state_path,
            state.desired_state,
            state.launch_spec.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[tokio::test]
    async fn new_discards_redacted_persisted_launch_spec() {
        let state_path = std::env::temp_dir().join(format!(
            "transport-ui-runtime-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let launch_spec = UiLaunchSpec {
            exec_path: "/tmp/electron".into(),
            args: vec!["main.js".into()],
            cwd: Some("/tmp".into()),
            env: HashMap::from([
                ("MANDO_AUTH_TOKEN".into(), "secret-token".into()),
                ("MANDO_GATEWAY_PORT".into(), "18701".into()),
            ]),
        };
        state_fs::persist_state(&state_path, UiDesiredState::Running, Some(launch_spec)).unwrap();

        let runtime = UiRuntime::new(state_path.clone());
        let status = runtime.status().await;

        assert_eq!(status.desired_state, UiDesiredState::Running);
        assert!(!status.launch_available);

        let _ = std::fs::remove_file(state_path);
    }
}
