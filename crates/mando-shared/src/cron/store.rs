//! Disk persistence for CronStore — JSON serialization/deserialization.
//!
//! Uses camelCase JSON keys to match the Python format.

use std::path::Path;

use mando_types::CronJob;
use serde::{Deserialize, Serialize};

/// Persistent store for cron jobs (disk format).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronStore {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub jobs: Vec<CronJob>,
}

fn default_version() -> u32 {
    1
}

impl Default for CronStore {
    fn default() -> Self {
        Self {
            version: 1,
            jobs: Vec::new(),
        }
    }
}

/// Load a `CronStore` from disk. Returns a default empty store on
/// missing file or parse error.
pub fn load_store(path: &Path) -> CronStore {
    crate::helpers::load_json_file(path, "cron_store")
}

/// Save a `CronStore` to disk as pretty-printed JSON.
pub fn save_store(store: &CronStore, path: &Path) -> anyhow::Result<()> {
    crate::helpers::save_json_file(path, store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_returns_default() {
        let store = load_store(Path::new("/tmp/does-not-exist-mando-test.json"));
        assert_eq!(store.version, 1);
        assert!(store.jobs.is_empty());
    }

    #[test]
    fn roundtrip_save_load() {
        let tmp = std::env::temp_dir().join("mando-shared-store-test.json");
        let store = CronStore {
            version: 1,
            jobs: vec![],
        };
        save_store(&store, &tmp).unwrap();
        let loaded = load_store(&tmp);
        assert_eq!(loaded.version, 1);
        assert!(loaded.jobs.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }
}
