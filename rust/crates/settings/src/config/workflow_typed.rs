//! Typed deserialization for workflow maps whose public API remains string-keyed.

use std::collections::HashMap;

use api_types::ItemStatus;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CaptainRolePrompts {
    worker: String,
    clarifier: String,
    interactive_clarifier: String,
    captain_review: String,
    rebase_worker: String,
    reopen_resume: String,
    reopen_context: String,
    review_reopen_message: String,
    mergeability_issue_draft: String,
    mergeability_issue_missing_evidence: String,
    mergeability_issue_stale_evidence: String,
    captain_merge: String,
}

#[derive(Debug, Deserialize)]
struct CaptainPromptTemplates {
    // Serde flatten cannot be combined with deny_unknown_fields. Any leftover
    // non-partial key is therefore rejected explicitly in `into_map` below.
    #[serde(flatten)]
    roles: CaptainRolePrompts,
    /// Shared include targets remain open-ended, but their `_` prefix keeps
    /// misspelled role prompts from being accepted as partials.
    #[serde(flatten)]
    partials: HashMap<String, String>,
}

impl CaptainPromptTemplates {
    fn into_map<E: serde::de::Error>(self) -> Result<HashMap<String, String>, E> {
        if let Some(name) = self.partials.keys().find(|name| !name.starts_with('_')) {
            return Err(E::custom(format!(
                "unknown role prompt `{name}`; shared partial names must start with `_`"
            )));
        }
        let mut templates = typed_templates_into_map::<_, E>(self.roles)?;
        templates.extend(self.partials);
        Ok(templates)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptainNudges {
    unresolved_threads: String,
    missing_work_summary: String,
    draft_pr: String,
    missing_evidence: String,
    stale_evidence: String,
    stale_work_summary: String,
    stream_stale: String,
    reopen_ack: String,
    nopr_insufficient_output: String,
    gates_incomplete: String,
    nudge_default: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptainInitialPrompts {
    worker: String,
    adopted: String,
}

fn typed_templates_into_map<T, E>(templates: T) -> Result<HashMap<String, String>, E>
where
    T: Serialize,
    E: serde::de::Error,
{
    let value = serde_yaml::to_value(templates).map_err(E::custom)?;
    serde_yaml::from_value(value).map_err(E::custom)
}

pub(super) fn deserialize_captain_prompts<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    CaptainPromptTemplates::deserialize(deserializer)?.into_map::<D::Error>()
}

pub(super) fn deserialize_captain_nudges<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let templates = CaptainNudges::deserialize(deserializer)?;
    typed_templates_into_map::<_, D::Error>(templates)
}

pub(super) fn deserialize_captain_initial_prompts<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let templates = CaptainInitialPrompts::deserialize(deserializer)?;
    typed_templates_into_map::<_, D::Error>(templates)
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct PerStateLimits(HashMap<ItemStatus, usize>);

pub(super) fn deserialize_per_state_limits<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, usize>, D::Error>
where
    D: Deserializer<'de>,
{
    let typed = PerStateLimits::deserialize(deserializer)?;
    let mut limits = HashMap::with_capacity(typed.0.len());
    for (status, limit) in typed.0 {
        let name = item_status_name::<D::Error>(status)?;
        ensure_live_session_status(status, &name).map_err(D::Error::custom)?;
        limits.insert(name, limit);
    }
    Ok(limits)
}

pub(super) fn validate_per_state_limit_key(key: &str) -> Result<(), String> {
    let status = serde_yaml::from_value::<ItemStatus>(serde_yaml::Value::String(key.to_owned()))
        .map_err(|_| format!("per_state_limits: unknown state '{key}'"))?;
    ensure_live_session_status(status, key)
}

fn ensure_live_session_status(status: ItemStatus, name: &str) -> Result<(), String> {
    if matches!(
        status,
        ItemStatus::InProgress
            | ItemStatus::Clarifying
            | ItemStatus::CaptainReviewing
            | ItemStatus::CaptainMerging
    ) {
        Ok(())
    } else {
        Err(format!(
            "per_state_limits: state '{name}' has no live agent session"
        ))
    }
}

fn item_status_name<E: serde::de::Error>(status: ItemStatus) -> Result<String, E> {
    let value = serde_yaml::to_value(status).map_err(E::custom)?;
    serde_yaml::from_value(value).map_err(E::custom)
}
