use crate::types::AppState;

pub fn single_project_repo(captain: &settings::config::settings::CaptainConfig) -> Option<String> {
    if captain.projects.len() == 1 {
        captain
            .projects
            .values()
            .next()
            .and_then(|project| project.github_repo.clone())
    } else {
        None
    }
}

pub fn captain_notifier(state: &AppState, config: &settings::config::Config) -> captain::Notifier {
    let default_slug = single_project_repo(&config.captain);
    captain::Notifier::new(state.bus.clone())
        .with_repo_slug(default_slug)
        .with_notifications_enabled(true)
}
