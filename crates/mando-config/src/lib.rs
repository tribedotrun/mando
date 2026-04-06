//! mando-config — configuration loading, env file parsing, and path constants.

pub mod error;
pub mod loader;
pub mod paths;
pub mod settings;
pub mod skills;
pub mod workflow;
pub mod workflow_scout;
pub mod workflow_validate;

// Convenience re-exports.
pub use error::ConfigError;
pub use loader::{get_config_path, load_config, save_config};
pub use paths::{
    active_captain_runtime_paths, bin_dir, captain_lock_path, cc_streams_dir, data_dir,
    detect_github_repo, expand_tilde, first_project_path, images_dir, logs_dir,
    match_project_by_prefix, parse_github_slug, resolve_captain_runtime_paths, resolve_github_repo,
    resolve_project_config, set_active_captain_runtime_paths, slugify, state_dir,
    stream_meta_path_for_session, stream_path_for_session, task_db_path, worker_health_path,
    CaptainRuntimePaths,
};
pub use settings::Config;
pub use workflow::{
    captain_workflow_path, load_captain_workflow, load_scout_workflow, render_initial_prompt,
    render_nudge, render_prompt, render_template, scout_workflow_path, try_load_captain_workflow,
    validate_template_syntax, AgentConfig, CaptainWorkflow, ModelsConfig, ScoutWorkflow,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_paths;
