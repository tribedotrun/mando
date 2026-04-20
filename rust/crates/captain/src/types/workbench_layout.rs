use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Persisted layout state for a workbench.
///
/// Stored as JSON at `~/.mando/workbenches/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkbenchLayout {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_panel: Option<String>,
    pub panel_order: Vec<String>,
    pub panels: HashMap<String, PanelState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    pub agent: String,
    pub created_at: u64,
}

impl WorkbenchLayout {
    pub fn new() -> Self {
        let now_ms = now_epoch_ms();
        let panel_id = "p1".to_string();
        let mut panels = HashMap::new();
        panels.insert(
            panel_id.clone(),
            PanelState {
                agent: "claude".to_string(),
                created_at: now_ms,
            },
        );
        Self {
            version: 1,
            active_panel: Some(panel_id.clone()),
            panel_order: vec![panel_id],
            panels,
        }
    }
}

impl Default for WorkbenchLayout {
    fn default() -> Self {
        Self::new()
    }
}

fn now_epoch_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as u64,
        Err(e) => global_infra::unrecoverable!("system clock before epoch", e),
    }
}
