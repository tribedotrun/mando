pub mod error;
pub mod loader;
pub mod logo;
pub mod paths;
pub mod settings;
pub mod skills;
pub mod workflow;
pub mod workflow_render;
pub mod workflow_scout;
pub mod workflow_validate;

pub use error::ConfigError;
pub use loader::{get_config_path, parse_config, serialize_config};
pub use logo::LOGO_CANDIDATES;
pub use paths::{
    detect_github_repo, first_project_path, match_project_by_prefix, parse_github_slug,
    resolve_github_repo, resolve_project_config,
};
pub use settings::{
    CaptainConfig, ChannelsConfig, ClassifyRule, Config, DashboardConfig, FeaturesConfig,
    GatewayConfig, ProjectConfig, ScoutConfig, TelegramConfig, UiConfig,
};
pub use skills::sync_bundled_skills;
pub use workflow::{
    captain_workflow_path, parse_captain_workflow_or_default, parse_scout_workflow_or_default,
    render_initial_prompt, render_nudge, render_prompt, render_template, scout_workflow_path,
    validate_template_syntax, AgentConfig, AutoTitleConfig, CaptainWorkflow, ModelsConfig,
    SandboxOverrides,
};
pub use workflow_scout::{
    InterestsConfig, ScoutAgentConfig, ScoutRepo, ScoutWorkflow, ScoutWorkflowOverride,
    UserContextConfig,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_paths;
