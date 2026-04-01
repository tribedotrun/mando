//! Scout workflow configuration types — interest profiles, user context,
//! repo context, and the top-level `ScoutWorkflow` struct.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Scout workflow configuration loaded from `scout-workflow.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutWorkflow {
    pub models: HashMap<String, String>,
    pub interests: InterestsConfig,
    pub repos: Vec<ScoutRepo>,
    pub user_context: UserContextConfig,
    pub prompts: HashMap<String, String>,
}

impl ScoutWorkflow {
    /// Load from compiled-in default YAML.
    /// Panics if the compiled-in asset is malformed — that is a build defect.
    pub fn compiled_default() -> Self {
        serde_yaml::from_str(DEFAULT_SCOUT_WORKFLOW)
            .expect("compiled scout-workflow.yaml is malformed — this is a build defect")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InterestsConfig {
    pub high: Vec<String>,
    pub medium: Vec<String>,
    pub low: Vec<String>,
    pub tone: String,
}

/// User context for scout prompts — adapts explanations to the reader's background.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserContextConfig {
    /// Reader's role/background.
    pub role: String,
    /// Domains the reader is expert in — terms here need no explanation.
    pub known_domains: Vec<String>,
    /// Domains outside the reader's expertise — terms here should be explained.
    pub explain_domains: Vec<String>,
}

impl UserContextConfig {
    /// Render user context as a formatted string for prompt injection.
    pub fn render(&self) -> String {
        if self.role.is_empty() && self.known_domains.is_empty() && self.explain_domains.is_empty()
        {
            return String::new();
        }

        let mut parts = Vec::new();
        if !self.role.is_empty() {
            parts.push(format!("Reader: {}", self.role));
        }
        if !self.known_domains.is_empty() {
            parts.push(format!(
                "Expert in (no need to explain basics): {}",
                self.known_domains.join(", ")
            ));
        }
        if !self.explain_domains.is_empty() {
            parts.push(format!(
                "Less familiar with (explain key terms): {}",
                self.explain_domains.join(", ")
            ));
        }
        parts.join("\n")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoutRepo {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub summary: String,
}

const DEFAULT_SCOUT_WORKFLOW: &str = include_str!("../assets/scout-workflow.yaml");
