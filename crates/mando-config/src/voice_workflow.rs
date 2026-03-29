//! Voice workflow configuration — loads voice-workflow.yaml.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceWorkflow {
    pub prompts: HashMap<String, String>,
}

impl VoiceWorkflow {
    /// Load from compiled-in default YAML.
    pub fn compiled_default() -> Self {
        serde_yaml::from_str(DEFAULT_VOICE_WORKFLOW).unwrap_or_else(|e| {
            tracing::error!("failed to parse compiled voice-workflow.yaml: {e}");
            Self::fallback()
        })
    }

    pub(crate) fn fallback() -> Self {
        Self {
            prompts: HashMap::new(),
        }
    }
}

impl Default for VoiceWorkflow {
    fn default() -> Self {
        Self::fallback()
    }
}

// ── Embedded default ─────────────────────────────────────────────────────────

const DEFAULT_VOICE_WORKFLOW: &str = include_str!("../assets/voice-workflow.yaml");

// ── Loading ──────────────────────────────────────────────────────────────────

fn parse_voice_workflow(yaml: &str) -> VoiceWorkflow {
    serde_yaml::from_str(yaml).unwrap_or_else(|e| {
        tracing::error!("failed to parse voice workflow.yaml: {e}");
        VoiceWorkflow::fallback()
    })
}

/// Load voice workflow: user override at `path` if it exists, else compiled-in default.
pub fn load_voice_workflow(override_path: &Path) -> VoiceWorkflow {
    if override_path.exists() {
        match std::fs::read_to_string(override_path) {
            Ok(contents) => {
                tracing::info!("loaded voice workflow from {}", override_path.display());
                return parse_voice_workflow(&contents);
            }
            Err(e) => {
                tracing::warn!(
                    "failed to read {}: {e} — using compiled-in default",
                    override_path.display()
                );
            }
        }
    }
    VoiceWorkflow::compiled_default()
}

// ── Path helper ──────────────────────────────────────────────────────────────

/// Default path for voice workflow override: `~/.mando/voice-workflow.yaml`.
pub fn voice_workflow_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("voice-workflow.yaml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_voice_workflow() {
        let wf = VoiceWorkflow::compiled_default();
        assert!(!wf.prompts.is_empty(), "should have prompts");
        assert!(
            wf.prompts.contains_key("voice_agent"),
            "should have voice_agent prompt"
        );
    }
}
