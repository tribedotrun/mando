//! Write-ahead intent log for repair operations.
//!
//! Before starting CC, write intent.json so orphaned repairs can be
//! recovered across restarts.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::info;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Intent {
    pub wt_path: String,
    pub branch: String,
    pub incident: serde_json::Value,
    pub status: IntentStatus,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    CcRunning,
    CcDone,
}

fn intent_path(state_dir: &Path) -> PathBuf {
    state_dir.join("intent.json")
}

pub(crate) fn write_intent(state_dir: &Path, intent: &Intent) -> Result<()> {
    std::fs::create_dir_all(state_dir)?;
    let json = serde_json::to_string_pretty(intent)?;
    std::fs::write(intent_path(state_dir), json)?;
    info!(
        module = "guardian",
        status = ?intent.status,
        branch = %intent.branch,
        "intent written"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) fn read_intent(state_dir: &Path) -> Option<Intent> {
    let path = intent_path(state_dir);
    let data = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&data) {
        Ok(intent) => Some(intent),
        Err(e) => {
            tracing::warn!(module = "guardian", error = %e, "corrupt intent file, removing");
            std::fs::remove_file(&path).ok();
            None
        }
    }
}

pub(crate) fn clear_intent(state_dir: &Path) {
    let path = intent_path(state_dir);
    if path.exists() {
        std::fs::remove_file(&path).ok();
        info!(module = "guardian", "intent cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_intent() {
        let dir = std::env::temp_dir().join("guardian-intent-test");
        std::fs::create_dir_all(&dir).unwrap();

        let intent = Intent {
            wt_path: "/tmp/wt".into(),
            branch: "self-improve/test".into(),
            incident: serde_json::json!({"message": "test error"}),
            status: IntentStatus::CcRunning,
            updated_at: "2025-01-01T00:00:00Z".into(),
        };

        write_intent(&dir, &intent).unwrap();
        let recovered = read_intent(&dir).unwrap();
        assert_eq!(recovered.status, IntentStatus::CcRunning);
        assert_eq!(recovered.branch, "self-improve/test");

        clear_intent(&dir);
        assert!(read_intent(&dir).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }
}
