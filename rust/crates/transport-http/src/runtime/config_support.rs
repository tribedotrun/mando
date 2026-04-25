use std::collections::HashMap;

fn wire_projects(
    config: &settings::Config,
) -> Result<HashMap<String, api_types::ProjectConfig>, serde_json::Error> {
    // Fail-fast: propagate serde errors instead of silently replacing a
    // project with `ProjectConfig::default()`, which previously blanked
    // the Settings UI for any project hit by schema drift.
    config
        .captain
        .projects
        .iter()
        .map(|(name, project)| {
            let value = serde_json::to_value(project)?;
            let wire: api_types::ProjectConfig = serde_json::from_value(value)?;
            Ok((name.clone(), wire))
        })
        .collect()
}

pub fn config_to_api(
    config: &settings::Config,
) -> Result<api_types::MandoConfig, serde_json::Error> {
    let mut value = serde_json::to_value(config)?;
    if let Some(captain) = value
        .get_mut("captain")
        .and_then(serde_json::Value::as_object_mut)
    {
        captain.insert(
            "projects".to_string(),
            serde_json::to_value(wire_projects(config)?)?,
        );
    }
    serde_json::from_value(value)
}
