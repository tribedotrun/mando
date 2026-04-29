use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use arc_swap::ArcSwap;
use sqlx::SqlitePool;
use tokio::sync::{watch, Mutex};

use super::runtime_helpers::{
    clamped_tick_duration, classify_change, hydrate_projects, load_workflows_for_mode,
    sync_process_env, validate_captain_workflow,
};
use crate::config::{scout_workflow_path, CaptainWorkflow, Config, ScoutWorkflow};
use crate::service::{
    apply_scout_workflow_mode_overrides, apply_workflow_mode_overrides, build_config_apply_outcome,
};
use crate::types::{ConfigApplyOutcome, ConfigChangeEvent, WorkflowRuntimeMode};

#[derive(Debug, thiserror::Error)]
pub enum ApplyConfigError {
    #[error("{0}")]
    Validation(String),
    #[error("workflow reload failed: {0}")]
    WorkflowReload(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Typed error returned from `SettingsRuntime` public methods per C2 (Issue #871).
/// `Other` shrinks as specific failure modes get promoted to dedicated variants.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Config(#[from] crate::config::error::ConfigError),
    #[error(transparent)]
    ApplyConfig(#[from] ApplyConfigError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Specialized alias for `SettingsRuntime` public API.
pub type SettingsResult<T> = std::result::Result<T, SettingsError>;

#[derive(Clone)]
pub struct SettingsRuntime {
    config: Arc<ArcSwap<Config>>,
    captain_workflow: Arc<ArcSwap<CaptainWorkflow>>,
    scout_workflow: Arc<ArcSwap<ScoutWorkflow>>,
    pub(crate) db_pool: SqlitePool,
    workflow_mode: WorkflowRuntimeMode,
    write_mu: Arc<Mutex<()>>,
    tick_tx: watch::Sender<Duration>,
}

impl SettingsRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn bootstrap(
        mut config: Config,
        db_pool: SqlitePool,
        workflow_mode: WorkflowRuntimeMode,
    ) -> SettingsResult<Self> {
        crate::io::projects::startup_sync(&db_pool, &mut config).await?;
        let (captain_workflow, scout_workflow) =
            load_workflows_for_mode(&mut config, workflow_mode)?;
        Ok(Self::new_with_loaded(
            config,
            captain_workflow,
            scout_workflow,
            db_pool,
            workflow_mode,
        ))
    }

    pub fn new_with_loaded(
        config: Config,
        captain_workflow: CaptainWorkflow,
        scout_workflow: ScoutWorkflow,
        db_pool: SqlitePool,
        workflow_mode: WorkflowRuntimeMode,
    ) -> Self {
        let (tick_tx, _) = watch::channel(clamped_tick_duration(
            config.captain.tick_interval_s,
            workflow_mode,
        ));
        Self {
            config: Arc::new(ArcSwap::from_pointee(config)),
            captain_workflow: Arc::new(ArcSwap::from_pointee(captain_workflow)),
            scout_workflow: Arc::new(ArcSwap::from_pointee(scout_workflow)),
            db_pool,
            workflow_mode,
            write_mu: Arc::new(Mutex::new(())),
            tick_tx,
        }
    }

    pub fn load_config(&self) -> Arc<Config> {
        self.config.load_full()
    }

    pub fn load_captain_workflow(&self) -> Arc<CaptainWorkflow> {
        self.captain_workflow.load_full()
    }

    pub fn load_scout_workflow(&self) -> Arc<ScoutWorkflow> {
        self.scout_workflow.load_full()
    }

    pub fn subscribe_tick(&self) -> watch::Receiver<Duration> {
        self.tick_tx.subscribe()
    }

    /// Re-read the captain workflow YAML at `override_path`, apply the
    /// current `WorkflowRuntimeMode` overrides, and atomically swap the
    /// in-memory `CaptainWorkflow`. On parse or validation failure, the
    /// previous in-memory copy is preserved and a warning is logged.
    /// Used by the captain auto-tick loop so operators can edit per-state
    /// caps, timeouts, prompts, or other workflow fields without
    /// restarting the gateway.
    ///
    /// `config.json` is not written. `apply_workflow_mode_overrides`
    /// runs against a discarded `Config` clone so any sandbox timing
    /// side-effects on `tick_interval_s` do not leak into the live
    /// config — that path is reserved for `apply_api_config` /
    /// `update_config`, which persist to disk. Returns `true` when the
    /// in-memory workflow was swapped, `false` when the previous copy
    /// was kept.
    #[tracing::instrument(skip_all)]
    pub async fn reload_captain_workflow_from_disk(&self, override_path: &Path) -> bool {
        let _guard = self.write_mu.lock().await;
        let mut config_clone = (*self.config.load_full()).clone();
        let mut captain_workflow = match crate::io::config_fs::try_load_captain_workflow(
            override_path,
            config_clone.captain.tick_interval_s,
        ) {
            Ok(wf) => wf,
            Err(err) => {
                tracing::warn!(
                    module = "settings-runtime-settings_runtime",
                    error = %err,
                    "captain workflow reload failed; keeping in-memory copy"
                );
                return false;
            }
        };
        let mut scout_workflow_clone = (*self.scout_workflow.load_full()).clone();
        // `apply_workflow_mode_overrides` re-validates the agent config
        // after Sandbox timing overrides via the panicking
        // `validate_agent_config`. Wrap in `catch_unwind` so an invalid
        // sandbox override (e.g. `stale_threshold_s` < 2 *
        // `tick_interval_s`) keeps the previous in-memory copy and logs
        // a warning rather than unwinding the auto-tick loop.
        let override_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply_workflow_mode_overrides(
                self.workflow_mode,
                &mut config_clone,
                &mut captain_workflow,
                &mut scout_workflow_clone,
            );
        }));
        if let Err(panic) = override_result {
            let msg = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "panic during apply_workflow_mode_overrides".into());
            tracing::warn!(
                module = "settings-runtime-settings_runtime",
                error = %msg,
                "captain workflow override re-validation failed; keeping in-memory copy"
            );
            return false;
        }
        self.captain_workflow.store(Arc::new(captain_workflow));
        true
    }

    #[tracing::instrument(skip_all)]
    pub async fn apply_api_config(
        &self,
        mut new_config: Config,
    ) -> Result<ConfigApplyOutcome, ApplyConfigError> {
        new_config.populate_runtime_fields();
        let _guard = self.write_mu.lock().await;
        let old_config = (*self.config.load_full()).clone();
        hydrate_projects(&self.db_pool, &old_config, &mut new_config).await;
        validate_captain_workflow(&new_config)?;
        let workflows = load_workflows_for_mode(&mut new_config, self.workflow_mode)
            .map_err(|err| ApplyConfigError::WorkflowReload(err.to_string()))?;
        let change = self
            .commit_locked(old_config, new_config, Some(workflows))
            .await?;
        Ok(build_config_apply_outcome(change, true, true))
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_config<F>(&self, mutator: F) -> SettingsResult<ConfigChangeEvent>
    where
        F: FnOnce(&mut Config) -> anyhow::Result<()>,
    {
        let _guard = self.write_mu.lock().await;
        let old_config = (*self.config.load_full()).clone();
        let mut new_config = old_config.clone();
        mutator(&mut new_config)?;
        new_config.populate_runtime_fields();
        hydrate_projects(&self.db_pool, &old_config, &mut new_config).await;
        let workflows = load_workflows_for_mode(&mut new_config, self.workflow_mode)?;
        self.commit_locked(old_config, new_config, Some(workflows))
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn sync_projects_from_db(&self) -> SettingsResult<ConfigChangeEvent> {
        let _guard = self.write_mu.lock().await;
        let old_config = (*self.config.load_full()).clone();
        let mut new_config = old_config.clone();
        crate::io::projects::load_into_config(&self.db_pool, &mut new_config).await?;
        let mut scout_workflow =
            crate::io::config_fs::load_scout_workflow(&scout_workflow_path(), &new_config)?;
        apply_scout_workflow_mode_overrides(self.workflow_mode, &mut scout_workflow);
        let captain_workflow = (*self.captain_workflow.load_full()).clone();
        self.commit_locked(
            old_config,
            new_config,
            Some((captain_workflow, scout_workflow)),
        )
        .await
        .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_projects(&self) -> SettingsResult<Vec<crate::io::projects::ProjectRow>> {
        crate::io::projects::list(&self.db_pool)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn resolve_project(
        &self,
        identifier: &str,
    ) -> SettingsResult<Option<crate::io::projects::ProjectRow>> {
        crate::io::projects::resolve(&self.db_pool, identifier)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_project_by_name(
        &self,
        name: &str,
    ) -> SettingsResult<Option<crate::io::projects::ProjectRow>> {
        crate::io::projects::find_by_name(&self.db_pool, name)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_project_by_path(
        &self,
        path: &str,
    ) -> SettingsResult<Option<crate::io::projects::ProjectRow>> {
        crate::io::projects::find_by_path(&self.db_pool, path)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn upsert_project(
        &self,
        row: &crate::io::projects::ProjectRow,
    ) -> SettingsResult<i64> {
        crate::io::projects::upsert_full(&self.db_pool, row)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_project(
        &self,
        id: i64,
        row: &crate::io::projects::ProjectRow,
    ) -> SettingsResult<bool> {
        crate::io::projects::update(&self.db_pool, id, row)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_project(&self, id: i64) -> SettingsResult<bool> {
        crate::io::projects::delete(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn is_git_repository(&self, path: &Path) -> SettingsResult<bool> {
        crate::io::git_repo::is_git_repository(path)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn detect_github_repo(&self, path: &str) -> Option<String> {
        crate::config::detect_github_repo(path).await
    }

    pub fn project_row_from_config(
        &self,
        config: &crate::config::settings::ProjectConfig,
    ) -> SettingsResult<crate::ProjectRow> {
        crate::io::projects::config_to_row(config)
            .map_err(|e| SettingsError::Other(anyhow::anyhow!(e)))
    }

    pub fn detect_project_logo(&self, project_path: &Path, project_name: &str) -> Option<String> {
        crate::io::logo::detect_project_logo(project_path, project_name)
    }

    async fn commit_locked(
        &self,
        old_config: Config,
        mut new_config: Config,
        workflows: Option<(CaptainWorkflow, ScoutWorkflow)>,
    ) -> anyhow::Result<ConfigChangeEvent> {
        new_config.populate_runtime_fields();

        let to_save = new_config.clone();
        tokio::task::spawn_blocking(move || crate::io::config_fs::save_config(&to_save, None))
            .await
            .context("config save task panicked")??;

        sync_process_env(&old_config.env, &new_config.env);

        self.config.store(Arc::new(new_config.clone()));
        if let Some((captain_workflow, scout_workflow)) = workflows {
            self.captain_workflow.store(Arc::new(captain_workflow));
            self.scout_workflow.store(Arc::new(scout_workflow));
        }

        if self
            .tick_tx
            .send(clamped_tick_duration(
                new_config.captain.tick_interval_s,
                self.workflow_mode,
            ))
            .is_err()
        {
            tracing::warn!(
                module = "config",
                "tick_tx has no receivers, tick interval change not propagated"
            );
        }

        Ok(classify_change(&old_config, &new_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    async fn make_runtime() -> SettingsRuntime {
        let db = global_db::Db::open_in_memory().await.unwrap();
        SettingsRuntime::new_with_loaded(
            Config::default(),
            CaptainWorkflow::compiled_default(),
            ScoutWorkflow::compiled_default(),
            db.pool().clone(),
            WorkflowRuntimeMode::Normal,
        )
    }

    /// Hot-reload smoke test: write a workflow yaml that bumps
    /// per_state_limits to a tempfile, call the reload setter, and
    /// confirm the in-memory captain workflow swaps to the new value.
    #[tokio::test]
    async fn reload_picks_up_new_per_state_limits() {
        let runtime = make_runtime().await;
        // Default ships with empty per_state_limits.
        assert!(runtime
            .load_captain_workflow()
            .agent
            .per_state_limits
            .is_empty());

        // Build YAML that overrides only the agent block plus the
        // mandatory template / nudge / stream-symptoms keys. Easiest
        // path: serialize the compiled default, mutate, write to disk.
        let mut wf = CaptainWorkflow::compiled_default();
        wf.agent.per_state_limits.insert("clarifying".into(), 7);
        let yaml = serde_yaml::to_string(&wf).expect("serialize wf");

        let tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp.as_file(), "{yaml}").unwrap();

        let swapped = runtime.reload_captain_workflow_from_disk(tmp.path()).await;
        assert!(swapped, "reload should succeed");
        let after = runtime.load_captain_workflow();
        assert_eq!(after.agent.per_state_limits.get("clarifying"), Some(&7));
    }

    /// On parse failure the reload setter must keep the previous
    /// in-memory copy and return false — operators editing the YAML
    /// shouldn't crash the daemon mid-edit.
    #[tokio::test]
    async fn reload_keeps_old_copy_on_parse_failure() {
        let runtime = make_runtime().await;
        let before_arc = runtime.load_captain_workflow();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp.as_file(), "this: is: not: valid: yaml: [").unwrap();

        let swapped = runtime.reload_captain_workflow_from_disk(tmp.path()).await;
        assert!(!swapped, "reload should fail and keep old copy");
        let after_arc = runtime.load_captain_workflow();
        // Same Arc pointer = no swap occurred.
        assert!(Arc::ptr_eq(&before_arc, &after_arc));
    }

    /// Sandbox-mode override re-validation must not crash the
    /// auto-tick loop. A workflow with a sandbox block whose timing
    /// overrides violate `validate_agent_config` (e.g. `stale_threshold_s`
    /// below 2 * the overridden `tick_interval_s`) panics inside
    /// `apply_workflow_mode_overrides`; the reload setter should catch
    /// that, log a warning, and keep the previous in-memory copy.
    #[tokio::test]
    async fn reload_keeps_old_copy_on_sandbox_override_panic() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let runtime = SettingsRuntime::new_with_loaded(
            Config::default(),
            CaptainWorkflow::compiled_default(),
            ScoutWorkflow::compiled_default(),
            db.pool().clone(),
            WorkflowRuntimeMode::Sandbox,
        );
        let before_arc = runtime.load_captain_workflow();

        // Sandbox tick_interval=2s but stale_threshold=1s violates the
        // 2x rule and trips `validate_agent_config` after the override
        // applies. Compiled-default top-level values are valid on their
        // own, so `try_load_captain_workflow` succeeds; the panic only
        // surfaces from the post-override re-validation.
        let mut wf = CaptainWorkflow::compiled_default();
        wf.sandbox.tick_interval_s = Some(2);
        wf.sandbox.stale_threshold_s = Some(1);
        let yaml = serde_yaml::to_string(&wf).expect("serialize wf");
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp.as_file(), "{yaml}").unwrap();

        let swapped = runtime.reload_captain_workflow_from_disk(tmp.path()).await;
        assert!(!swapped, "reload should reject panicking sandbox override");
        let after_arc = runtime.load_captain_workflow();
        assert!(
            Arc::ptr_eq(&before_arc, &after_arc),
            "in-memory workflow must stay on the previous Arc"
        );
    }
}
