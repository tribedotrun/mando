use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use settings::Config;

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
    let defaults = default_captain_runtime_paths();
    CaptainRuntimePaths {
        task_db_path: config_path_or_default(&config.captain.task_db_path, defaults.task_db_path),
        lockfile_path: config_path_or_default(
            &config.captain.lockfile_path,
            defaults.lockfile_path,
        ),
        worker_health_path: config_path_or_default(
            &config.captain.worker_health_path,
            defaults.worker_health_path,
        ),
    }
}

fn config_path_or_default(value: &str, default: PathBuf) -> PathBuf {
    if value.trim().is_empty() {
        default
    } else {
        global_infra::paths::expand_tilde(value)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_skipped_config_paths_fall_back_to_runtime_defaults() {
        let mut config = Config::default();
        config.captain.task_db_path.clear();
        config.captain.lockfile_path.clear();
        config.captain.worker_health_path.clear();

        let paths = resolve_captain_runtime_paths(&config);

        assert!(paths.task_db_path.ends_with("mando.db"));
        assert!(paths.lockfile_path.ends_with("captain.lock"));
        assert!(paths
            .worker_health_path
            .ends_with("state/worker-health.json"));
    }

    #[test]
    fn explicit_config_paths_are_honored() {
        let mut config = Config::default();
        config.captain.task_db_path = "/tmp/custom.db".into();
        config.captain.lockfile_path = "/tmp/custom.lock".into();
        config.captain.worker_health_path = "/tmp/custom-health.json".into();

        let paths = resolve_captain_runtime_paths(&config);

        assert_eq!(paths.task_db_path, PathBuf::from("/tmp/custom.db"));
        assert_eq!(paths.lockfile_path, PathBuf::from("/tmp/custom.lock"));
        assert_eq!(
            paths.worker_health_path,
            PathBuf::from("/tmp/custom-health.json")
        );
    }
}
