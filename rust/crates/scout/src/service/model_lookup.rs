//! Required-model lookup for scout workflow.
//!
//! Returns an error if a named model is missing instead of silently defaulting
//! to an empty string (which would produce a malformed CC call and confuse the
//! model router).

use settings::ScoutWorkflow;

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
