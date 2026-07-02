use std::path::Path;

use super::ConfigError;

/// Deep-merge a user workflow overlay onto the compiled default. This keeps
/// older `~/.mando/workflow.yaml` overrides bootable when new required
/// sections are added, without relying on serde fallback defaults. Objects
/// recurse key-by-key; `null` is a no-op so a stale override cannot erase a
/// newly-required default subtree.
pub(super) fn merge_captain_workflow_override(
    default_yaml: &str,
    override_yaml: &str,
    path: &Path,
) -> Result<serde_yaml::Value, ConfigError> {
    let default_value = parse_yaml_value(default_yaml, path)?;
    let override_value = parse_yaml_value(override_yaml, path)?;
    Ok(merge_yaml_value(default_value, override_value))
}

fn parse_yaml_value(yaml: &str, path: &Path) -> Result<serde_yaml::Value, ConfigError> {
    serde_yaml::from_str(yaml).map_err(|e| ConfigError::YamlParse {
        path: path.to_path_buf(),
        source: e,
    })
}

fn merge_yaml_value(base: serde_yaml::Value, overlay: serde_yaml::Value) -> serde_yaml::Value {
    match (base, overlay) {
        (base, serde_yaml::Value::Null) => base,
        (serde_yaml::Value::Mapping(mut base_map), serde_yaml::Value::Mapping(overlay_map)) => {
            for (key, value) in overlay_map {
                if matches!(value, serde_yaml::Value::Null) {
                    continue;
                }
                let merged = match base_map.remove(&key) {
                    Some(existing) => merge_yaml_value(existing, value),
                    None => value,
                };
                base_map.insert(key, merged);
            }
            serde_yaml::Value::Mapping(base_map)
        }
        (_, overlay) => overlay,
    }
}
