use std::collections::HashMap;

pub(crate) fn config_from_api(config: api_types::MandoConfig) -> settings::Config {
    let api_types::MandoConfig {
        workspace,
        ui,
        features,
        channels,
        gateway,
        captain,
        scout,
        env,
    } = config;
    let api_types::UiConfig { open_at_login } = ui;
    let api_types::FeaturesConfig {
        scout: scout_enabled,
        setup_dismissed,
        claude_code_verified,
    } = features;
    let api_types::ChannelsConfig { telegram } = channels;
    let api_types::TelegramConfig { enabled, owner } = telegram;
    let api_types::GatewayConfig { dashboard } = gateway;
    let api_types::DashboardConfig { host, port } = dashboard;
    let api_types::CaptainConfig {
        auto_schedule,
        auto_merge,
        max_concurrent_workers,
        tick_interval_s,
        tz,
        default_task_agent,
        default_glm_implementation,
        // Config writes cannot mutate projects; the project endpoints are their only writers.
        projects: _,
    } = captain;
    let api_types::ScoutConfig {
        interests,
        user_context,
    } = scout;
    let api_types::InterestsConfig { high, low } = interests;
    let api_types::UserContextConfig {
        role,
        known_domains,
        explain_domains,
    } = user_context;
    let captain_defaults = settings::CaptainConfig::default();

    settings::Config {
        workspace,
        ui: settings::UiConfig { open_at_login },
        features: settings::FeaturesConfig {
            scout: scout_enabled,
            setup_dismissed,
            claude_code_verified,
        },
        channels: settings::ChannelsConfig {
            telegram: settings::TelegramConfig {
                enabled,
                token: String::new(),
                owner,
            },
        },
        gateway: settings::GatewayConfig {
            dashboard: settings::DashboardConfig { host, port },
        },
        captain: settings::CaptainConfig {
            auto_schedule,
            auto_merge,
            max_concurrent_workers,
            tick_interval_s,
            tz,
            default_task_agent,
            default_glm_implementation,
            projects: HashMap::new(),
            task_db_path: captain_defaults.task_db_path,
            lockfile_path: captain_defaults.lockfile_path,
            worker_health_path: captain_defaults.worker_health_path,
        },
        scout: settings::ScoutConfig {
            interests: settings::InterestsConfig { high, low },
            user_context: settings::UserContextConfig {
                role,
                known_domains,
                explain_domains,
            },
        },
        env,
    }
}

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
