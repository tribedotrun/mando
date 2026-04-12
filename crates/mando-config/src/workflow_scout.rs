//! Scout workflow configuration types — interest profiles, user context,
//! repo context, and the top-level `ScoutWorkflow` struct.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Scout workflow configuration loaded from `scout-workflow.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutWorkflow {
    pub models: HashMap<String, String>,
    pub agent: ScoutAgentConfig,
    pub interests: InterestsConfig,
    pub repos: Vec<ScoutRepo>,
    pub user_context: UserContextConfig,
    pub prompts: HashMap<String, String>,
}

/// Serde adapter that reads/writes a `Duration` as a floating-point seconds value.
mod duration_seconds {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.as_secs_f64().serialize(s)
    }

    const MAX_DURATION_SECS: f64 = 1e18;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = f64::deserialize(d)?;
        if !secs.is_finite() || secs > MAX_DURATION_SECS {
            return Err(serde::de::Error::custom(format!(
                "duration must be finite and <= {MAX_DURATION_SECS:e} seconds, got {secs}"
            )));
        }
        Ok(Duration::from_secs_f64(secs.max(0.0)))
    }
}

/// Timeout configuration for scout CC sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutAgentConfig {
    #[serde(with = "duration_seconds")]
    pub process_timeout_s: Duration,
    #[serde(with = "duration_seconds")]
    pub article_timeout_s: Duration,
    #[serde(with = "duration_seconds")]
    pub research_timeout_s: Duration,
    #[serde(with = "duration_seconds")]
    pub qa_timeout_s: Duration,
    #[serde(with = "duration_seconds")]
    pub qa_ttl_s: Duration,
    #[serde(with = "duration_seconds")]
    pub act_timeout_s: Duration,
    /// Max links to accept from a single research run.
    #[serde(default = "default_research_max_items")]
    pub research_max_items: usize,
}

fn default_research_max_items() -> usize {
    10
}

impl Default for ScoutAgentConfig {
    fn default() -> Self {
        Self {
            process_timeout_s: Duration::from_secs(120),
            article_timeout_s: Duration::from_secs(300),
            research_timeout_s: Duration::from_secs(1800),
            qa_timeout_s: Duration::from_secs(120),
            qa_ttl_s: Duration::from_secs(600),
            act_timeout_s: Duration::from_secs(60),
            research_max_items: default_research_max_items(),
        }
    }
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
    pub low: Vec<String>,
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
