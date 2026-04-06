//! Required-model lookup for scout workflow.
//!
//! Returns an error if a named model is missing instead of silently defaulting
//! to an empty string (which would produce a malformed CC call and confuse the
//! model router).

use mando_config::ScoutWorkflow;

/// Fetch a required model by key. Returns a descriptive error if missing or
/// empty so callers can propagate with `?` instead of defaulting.
pub fn required_model(workflow: &ScoutWorkflow, key: &str) -> anyhow::Result<String> {
    match workflow.models.get(key) {
        Some(model) if !model.is_empty() => Ok(model.clone()),
        Some(_) => Err(anyhow::anyhow!(
            "scout workflow model '{key}' is configured but empty — check scout-workflow.yaml"
        )),
        None => Err(anyhow::anyhow!(
            "scout workflow model '{key}' missing — expected in scout-workflow.yaml models map"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn workflow_with(models: &[(&str, &str)]) -> ScoutWorkflow {
        ScoutWorkflow {
            models: models
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<HashMap<_, _>>(),
            ..ScoutWorkflow::default()
        }
    }

    #[test]
    fn returns_model_when_present() {
        let wf = workflow_with(&[("article", "sonnet")]);
        assert_eq!(required_model(&wf, "article").unwrap(), "sonnet");
    }

    #[test]
    fn errors_when_missing() {
        let wf = workflow_with(&[]);
        let err = required_model(&wf, "qa").unwrap_err().to_string();
        assert!(err.contains("qa"), "{err}");
    }

    #[test]
    fn errors_when_empty() {
        let wf = workflow_with(&[("process", "")]);
        let err = required_model(&wf, "process").unwrap_err().to_string();
        assert!(err.contains("process"), "{err}");
    }
}
