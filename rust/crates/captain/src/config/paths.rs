use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use settings::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptainRuntimePaths {
    pub task_db_path: PathBuf,
    pub lockfile_path: PathBuf,
    pub worker_health_path: PathBuf,
}

fn default_captain_runtime_paths() -> CaptainRuntimePaths {
    CaptainRuntimePaths {
        task_db_path: global_infra::paths::data_dir().join("mando.db"),
        lockfile_path: global_infra::paths::data_dir().join("captain.lock"),
        worker_health_path: global_infra::paths::state_dir().join("worker-health.json"),
    }
}

fn captain_runtime_paths_cell() -> &'static RwLock<Option<CaptainRuntimePaths>> {
    static CELL: OnceLock<RwLock<Option<CaptainRuntimePaths>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(None))
}

fn read_runtime_paths() -> Option<CaptainRuntimePaths> {
    captain_runtime_paths_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn resolve_captain_runtime_paths(config: &Config) -> CaptainRuntimePaths {
    CaptainRuntimePaths {
        task_db_path: global_infra::paths::expand_tilde(&config.captain.task_db_path),
        lockfile_path: global_infra::paths::expand_tilde(&config.captain.lockfile_path),
        worker_health_path: global_infra::paths::expand_tilde(&config.captain.worker_health_path),
    }
}

pub fn set_active_captain_runtime_paths(paths: CaptainRuntimePaths) {
    *captain_runtime_paths_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(paths);
}

fn active_captain_runtime_paths() -> CaptainRuntimePaths {
    read_runtime_paths().unwrap_or_else(default_captain_runtime_paths)
}

pub fn captain_lock_path() -> PathBuf {
    active_captain_runtime_paths().lockfile_path
}

pub fn worker_health_path() -> PathBuf {
    active_captain_runtime_paths().worker_health_path
}
