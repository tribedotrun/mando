use serde_json::Value;

pub fn inject_projects(config: &settings::config::Config, val: &mut Value) {
    if let Some(captain) = val.get_mut("captain") {
        let projects: serde_json::Map<String, Value> = config
            .captain
            .projects
            .iter()
            .map(|(key, project)| {
                (
                    key.clone(),
                    serde_json::to_value(project).unwrap_or_default(),
                )
            })
            .collect();
        captain["projects"] = Value::Object(projects);
    }
}
