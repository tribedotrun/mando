//! Async CronService — runtime cron scheduler using tokio.
//!
//! Loads jobs from DB, arms a timer for the next due job, executes
//! callbacks, and persists state changes.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mando_types::{
    CronJob, CronPayload, CronSchedule, CronState, JobType, PayloadKind, ScheduleKind,
};
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use tracing::{error, info};

use super::scheduler::compute_next_run;

/// Callback type for job execution.
pub type JobCallback =
    Arc<dyn Fn(CronJob) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync>;

/// Async cron service that schedules and executes jobs.
pub struct CronService {
    pool: SqlitePool,
    jobs: Vec<CronJob>,
    on_job: Option<JobCallback>,
    timer_handle: Option<JoinHandle<()>>,
    running: bool,
}

impl CronService {
    /// Create a new CronService backed by the given DB pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            jobs: Vec::new(),
            on_job: None,
            timer_handle: None,
            running: false,
        }
    }

    /// Load jobs from DB, recompute next runs, arm timer.
    ///
    /// On first run after upgrade, imports any legacy file-based jobs from
    /// `~/.mando/state/cron/jobs.json` into the DB, then renames the file.
    pub async fn start(&mut self) {
        self.running = true;
        self.jobs = match mando_db::queries::cron::load_all(&self.pool).await {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("cron service failed to load jobs from DB: {e}");
                Vec::new()
            }
        };

        // One-time migration: import legacy file-based cron jobs into DB.
        if self.jobs.is_empty() {
            let legacy_path = mando_config::cron_store_path();
            if legacy_path.exists() {
                let legacy = super::store::load_store(&legacy_path);
                if !legacy.jobs.is_empty() {
                    info!(
                        "cron: migrating {} legacy jobs from {}",
                        legacy.jobs.len(),
                        legacy_path.display()
                    );
                    self.jobs = legacy.jobs;
                    if let Err(e) = self.save().await {
                        error!("cron: failed to persist migrated legacy jobs: {e}");
                    } else {
                        let backup = legacy_path.with_extension("json.migrated");
                        if let Err(e) = std::fs::rename(&legacy_path, &backup) {
                            error!("cron: failed to rename legacy file: {e}");
                        } else {
                            info!("cron: legacy store renamed to {}", backup.display());
                        }
                    }
                }
            }
        }

        self.recompute_next_runs();
        if let Err(e) = self.save().await {
            error!("cron service failed to save jobs during start: {e}");
        }
        self.arm_timer();
        info!("cron service started with {} jobs", self.jobs.len());
    }

    /// Stop scheduling. Does not cancel in-flight jobs.
    pub fn stop(&mut self) {
        self.running = false;
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }
    }

    /// Set the callback that runs when a job fires.
    /// Must be called before `start()` — `arm_timer()` skips scheduling
    /// if no callback is registered.
    pub fn set_on_job(&mut self, cb: JobCallback) {
        self.on_job = Some(cb);
    }

    /// List jobs, optionally including disabled ones.
    pub fn list_jobs(&self, include_disabled: bool) -> Vec<&CronJob> {
        let mut jobs: Vec<&CronJob> = self
            .jobs
            .iter()
            .filter(|j| include_disabled || j.enabled)
            .collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    /// Add a new job. Returns the created job.
    pub async fn add_job(
        &mut self,
        id: String,
        name: String,
        schedule: CronSchedule,
        message: String,
        now_ms: i64,
    ) -> Result<CronJob, String> {
        let next = compute_next_run(&schedule, now_ms);
        let job = CronJob {
            id,
            name,
            enabled: true,
            schedule,
            payload: CronPayload {
                kind: PayloadKind::AgentTurn,
                message,
                deliver: false,
                channel: None,
                to: None,
            },
            state: CronState {
                next_run_at_ms: next,
                last_run_at_ms: None,
                last_status: None,
                last_error: None,
            },
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            delete_after_run: false,
            job_type: JobType::System,
            cwd: None,
            timeout_s: 1200,
        };
        self.jobs.push(job.clone());
        self.save().await?;
        self.arm_timer();
        Ok(job)
    }

    /// Remove a job by ID. Returns true if found and removed.
    pub async fn remove_job(&mut self, id: &str) -> Result<bool, String> {
        let before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        let removed = self.jobs.len() < before;
        if removed {
            self.save().await?;
            self.arm_timer();
            info!("cron: removed job {id}");
        }
        Ok(removed)
    }

    /// Enable or disable a job. Returns the updated job reference.
    pub async fn toggle_job(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<&CronJob>, String> {
        let job = match self.jobs.iter_mut().find(|j| j.id == id) {
            Some(j) => j,
            None => return Ok(None),
        };
        let now = now_ms();
        job.enabled = enabled;
        job.updated_at_ms = now;
        if enabled {
            job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
        } else {
            job.state.next_run_at_ms = None;
        }
        self.save().await?;
        self.arm_timer();
        // Re-borrow immutably.
        Ok(self.jobs.iter().find(|j| j.id == id))
    }

    /// Manually run a job by ID.
    pub async fn run_job(&mut self, id: &str) -> Result<bool, String> {
        let job = self
            .jobs
            .iter()
            .find(|j| j.id == id)
            .cloned()
            .ok_or_else(|| format!("job not found: {id}"))?;
        self.execute_job(&job).await;
        self.save().await?;
        self.arm_timer();
        Ok(true)
    }

    // ---------------------------------------------------------------
    // Internal
    // ---------------------------------------------------------------

    fn recompute_next_runs(&mut self) {
        let now = now_ms();
        for job in &mut self.jobs {
            if job.enabled {
                job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
            }
        }
    }

    fn get_next_wake_ms(&self) -> Option<i64> {
        self.jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.state.next_run_at_ms)
            .min()
    }

    fn arm_timer(&mut self) {
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }
        if !self.running {
            return;
        }

        let next_ms = match self.get_next_wake_ms() {
            Some(ms) => ms,
            None => return,
        };

        let delay_ms = (next_ms - now_ms()).max(0) as u64;
        let cb = match self.on_job.clone() {
            Some(cb) => cb,
            None => return,
        };

        // Find the job that fires at this time.
        let job = self
            .jobs
            .iter()
            .find(|j| j.enabled && j.state.next_run_at_ms == Some(next_ms))
            .cloned();

        let pool = self.pool.clone();

        self.timer_handle = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            if let Some(mut job) = job {
                info!("cron: firing job '{}' ({})", job.name, job.id);
                let cb_result = cb(job.clone()).await;
                if let Err(ref e) = cb_result {
                    error!("cron: job '{}' callback failed: {e}", job.name);
                }
                // Update job state in DB after execution — reuse original `job`.
                let now = now_ms();
                job.state.last_run_at_ms = Some(now);
                match &cb_result {
                    Ok(()) => {
                        job.state.last_status = Some("ok".into());
                        job.state.last_error = None;
                    }
                    Err(e) => {
                        job.state.last_status = Some("error".into());
                        job.state.last_error = Some(e.clone());
                    }
                }
                if job.schedule.kind == ScheduleKind::At {
                    job.enabled = false;
                    job.state.next_run_at_ms = None;
                } else {
                    job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
                }
                if let Err(e) = mando_db::queries::cron::upsert(&pool, &job).await {
                    error!("cron: failed to persist job state after execution: {e}");
                }
            }
        }));
    }

    async fn execute_job(&mut self, job: &CronJob) {
        let start = now_ms();
        info!("cron: executing job '{}' ({})", job.name, job.id);

        let result = if let Some(ref cb) = self.on_job {
            cb(job.clone()).await
        } else {
            Err("no job callback registered".to_string())
        };

        // Update job state in the in-memory store.
        if let Some(stored) = self.jobs.iter_mut().find(|j| j.id == job.id) {
            match result {
                Ok(()) => {
                    stored.state.last_status = Some("ok".into());
                    stored.state.last_error = None;
                }
                Err(e) => {
                    stored.state.last_status = Some("error".into());
                    stored.state.last_error = Some(e.clone());
                    error!("cron: job '{}' failed: {e}", job.name);
                }
            }
            stored.state.last_run_at_ms = Some(start);
            stored.updated_at_ms = now_ms();

            if stored.schedule.kind == ScheduleKind::At {
                if stored.delete_after_run {
                    let id = stored.id.clone();
                    self.jobs.retain(|j| j.id != id);
                } else {
                    stored.enabled = false;
                    stored.state.next_run_at_ms = None;
                }
            } else {
                stored.state.next_run_at_ms = compute_next_run(&stored.schedule, now_ms());
            }
        }
    }

    async fn save(&self) -> Result<(), String> {
        mando_db::queries::cron::replace_all(&self.pool, &self.jobs)
            .await
            .map_err(|e| format!("failed to save cron jobs to DB: {e}"))
    }
}

/// Current time in milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    #[tokio::test]
    async fn service_add_and_list() {
        let pool = test_pool().await;
        let mut svc = CronService::new(pool);
        svc.start().await;

        let schedule = CronSchedule {
            kind: ScheduleKind::Every,
            every_ms: Some(60_000),
            ..CronSchedule::default()
        };
        let now = now_ms();
        svc.add_job(
            "test-1".into(),
            "Test Job".into(),
            schedule,
            "hello".into(),
            now,
        )
        .await
        .unwrap();

        let jobs = svc.list_jobs(false);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "Test Job");

        svc.stop();
    }

    #[tokio::test]
    async fn service_remove_job() {
        let pool = test_pool().await;
        let mut svc = CronService::new(pool);
        svc.start().await;

        let schedule = CronSchedule {
            kind: ScheduleKind::Every,
            every_ms: Some(60_000),
            ..CronSchedule::default()
        };
        svc.add_job(
            "rm-1".into(),
            "To Remove".into(),
            schedule,
            "msg".into(),
            now_ms(),
        )
        .await
        .unwrap();

        assert!(svc.remove_job("rm-1").await.unwrap());
        assert!(!svc.remove_job("rm-1").await.unwrap()); // already gone
        assert!(svc.list_jobs(true).is_empty());

        svc.stop();
    }

    #[tokio::test]
    async fn service_toggle_job() {
        let pool = test_pool().await;
        let mut svc = CronService::new(pool);
        svc.start().await;

        let schedule = CronSchedule {
            kind: ScheduleKind::Every,
            every_ms: Some(60_000),
            ..CronSchedule::default()
        };
        svc.add_job(
            "tg-1".into(),
            "Toggle".into(),
            schedule,
            "msg".into(),
            now_ms(),
        )
        .await
        .unwrap();

        let job = svc.toggle_job("tg-1", false).await.unwrap().unwrap();
        assert!(!job.enabled);
        assert!(job.state.next_run_at_ms.is_none());

        let job = svc.toggle_job("tg-1", true).await.unwrap().unwrap();
        assert!(job.enabled);
        assert!(job.state.next_run_at_ms.is_some());

        svc.stop();
    }
}
