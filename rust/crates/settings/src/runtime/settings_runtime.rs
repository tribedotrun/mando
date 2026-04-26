use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use arc_swap::ArcSwap;
use sqlx::SqlitePool;
use tokio::sync::{watch, Mutex};

use super::runtime_helpers::{clamped_tick_duration, classify_change, sync_process_env};
use crate::config::{
    captain_workflow_path, scout_workflow_path, CaptainWorkflow, Config, ScoutWorkflow,
};
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
    pub async fn list_credentials(&self) -> Vec<crate::io::credentials::CredentialInfo> {
        match crate::io::credentials::list_all(&self.db_pool).await {
            Ok(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut info = row.to_info();
                    if let Some(last) = row.last_probed_at {
                        let cost =
                            crate::io::credentials::cost_since(&self.db_pool, row.id, last).await;
                        if cost > 0.0 {
                            info.cost_since_probe_usd = Some(cost);
                        }
                    }
                    out.push(info);
                }
                out
            }
            Err(err) => {
                tracing::warn!(module = "credentials", error = %err, "failed to list credentials");
                Vec::new()
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_credential_token(&self, id: i64) -> SettingsResult<Option<String>> {
        crate::io::credentials::get_token_by_id(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_credential_row(
        &self,
        id: i64,
    ) -> SettingsResult<Option<crate::io::credentials::CredentialRow>> {
        crate::io::credentials::get_row_by_id(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn store_credential(
        &self,
        label: &str,
        access_token: &str,
        expires_at: Option<i64>,
    ) -> SettingsResult<i64> {
        let id =
            crate::io::credentials::insert(&self.db_pool, label, access_token, expires_at).await?;
        tracing::info!(module = "credentials", id, "stored credential");
        Ok(id)
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_credential(&self, id: i64) -> SettingsResult<bool> {
        let removed = crate::io::credentials::delete(&self.db_pool, id).await?;
        if removed {
            tracing::info!(module = "credentials", id, "removed credential");
        }
        Ok(removed)
    }

    #[tracing::instrument(skip_all)]
    pub async fn mark_credential_expired(&self, id: i64) -> SettingsResult<bool> {
        crate::io::credentials::mark_expired(&self.db_pool, id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn has_any_credentials(&self) -> SettingsResult<bool> {
        crate::io::credentials::has_any(&self.db_pool)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn pick_worker_credential(
        &self,
        caller_filter: Option<&str>,
    ) -> SettingsResult<Option<(i64, String)>> {
        crate::io::credentials::pick_for_worker(&self.db_pool, caller_filter)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn earliest_credential_cooldown_remaining_secs(&self) -> SettingsResult<i64> {
        // Wrap the anyhow error from the io layer into the typed
        // SettingsError envelope so the public SettingsRuntime API
        // stays C2-compliant (no raw anyhow on the boundary).
        crate::io::credentials::earliest_cooldown_remaining_secs(&self.db_pool)
            .await
            .map_err(SettingsError::Other)
    }

    #[tracing::instrument(skip_all)]
    pub async fn credential_labels_by_ids(
        &self,
        ids: &[i64],
    ) -> SettingsResult<HashMap<i64, String>> {
        crate::io::credentials::labels_by_ids(&self.db_pool, ids)
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

async fn hydrate_projects(db_pool: &SqlitePool, old_config: &Config, new_config: &mut Config) {
    if let Err(err) = crate::io::projects::load_into_config(db_pool, new_config).await {
        tracing::warn!(module = "config", error = %err, "failed to reload projects after config save");
        new_config.captain.projects = old_config.captain.projects.clone();
    }
}

fn validate_captain_workflow(config: &Config) -> Result<(), ApplyConfigError> {
    crate::io::config_fs::try_load_captain_workflow(
        &captain_workflow_path(),
        config.captain.tick_interval_s,
    )
    .map(|_| ())
    .map_err(|err| ApplyConfigError::Validation(err.to_string()))
}

fn load_workflows_for_mode(
    config: &mut Config,
    workflow_mode: WorkflowRuntimeMode,
) -> anyhow::Result<(CaptainWorkflow, ScoutWorkflow)> {
    let mut captain_workflow = crate::io::config_fs::load_captain_workflow(
        &captain_workflow_path(),
        config.captain.tick_interval_s,
    )?;
    let mut scout_workflow =
        crate::io::config_fs::load_scout_workflow(&scout_workflow_path(), config)?;
    apply_workflow_mode_overrides(
        workflow_mode,
        config,
        &mut captain_workflow,
        &mut scout_workflow,
    );
    match workflow_mode {
        WorkflowRuntimeMode::Normal => {}
        WorkflowRuntimeMode::Dev => tracing::info!(
            module = "settings-runtime-settings_runtime",
            "dev mode: all models forced to sonnet"
        ),
        WorkflowRuntimeMode::Sandbox => tracing::info!(
            module = "settings-runtime-settings_runtime",
            tick_interval_s = config.captain.tick_interval_s,
            stale_threshold_s = captain_workflow.agent.stale_threshold_s.as_secs(),
            "sandbox mode: models forced to haiku + timing overrides applied"
        ),
    }
    Ok((captain_workflow, scout_workflow))
}
