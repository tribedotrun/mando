//! Picker state persistence — save/load to `~/.mando/state/picker-state.json`.

use std::collections::HashMap;

use tracing::info;

use crate::bot::{PickerItem, PickerState};

/// Serialize picker map to JSON.
pub(crate) fn collect_json(action: &HashMap<String, PickerState>) -> serde_json::Value {
    serde_json::json!({
        "action": serialize_map(action),
    })
}

/// Restore picker map from JSON.
pub(crate) fn restore_json(val: &serde_json::Value) -> PickerMaps {
    PickerMaps {
        action: restore_map(&val["action"]),
    }
}

pub(crate) struct PickerMaps {
    pub action: HashMap<String, PickerState>,
}

/// Save picker state to disk.
pub(crate) fn save(json: &serde_json::Value) {
    let path = mando_config::state_dir().join("picker-state.json");
    match serde_json::to_string_pretty(json) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                tracing::warn!(module = "picker", path = %path.display(), error = %e, "failed to persist picker state");
            }
        }
        Err(e) => {
            tracing::warn!(module = "picker", error = %e, "failed to serialize picker state");
        }
    }
}

/// Load picker state from disk. Returns None if file doesn't exist.
pub(crate) fn load() -> Option<PickerMaps> {
    let path = mando_config::state_dir().join("picker-state.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    info!("loaded picker state from disk");
    Some(restore_json(&val))
}

fn serialize_map(m: &HashMap<String, PickerState>) -> serde_json::Value {
    let entries: HashMap<String, serde_json::Value> = m
        .iter()
        .map(|(k, ps)| {
            (
                k.clone(),
                serde_json::json!({
                    "chat_id": ps.chat_id,
                    "items": ps.items,
                    "selected": ps.selected.iter().copied().collect::<Vec<usize>>(),
                }),
            )
        })
        .collect();
    serde_json::to_value(entries).unwrap_or_default()
}

fn restore_map(v: &serde_json::Value) -> HashMap<String, PickerState> {
    let mut m = HashMap::new();
    if let Some(obj) = v.as_object() {
        for (k, entry) in obj {
            let chat_id = entry["chat_id"].as_str().unwrap_or("").to_string();
            let items: Vec<PickerItem> = entry["items"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|i| serde_json::from_value(i.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();
            let selected: std::collections::HashSet<usize> = entry["selected"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as usize))
                        .collect()
                })
                .unwrap_or_default();
            m.insert(
                k.clone(),
                PickerState {
                    chat_id,
                    items,
                    selected,
                },
            );
        }
    }
    m
}
