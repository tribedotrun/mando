//! The enforced JSON schema is what actually narrows verdicts per trigger —
//! the rewritten prompt states one verdict list and defers the gating here.
//! These tests are therefore the sole guard on per-trigger availability.

use super::*;

#[test]
fn test_verdict_schema_is_trigger_aware() {
    // Default triggers (gates_pass, repeated_nudge, timeout,
    // degraded_context, rebase_fail, ci_failure, merge_fail, retry): ship,
    // nudge, respawn, reset_budget, and escalate.
    for trigger in [
        "gates_pass",
        "repeated_nudge",
        "timeout",
        "degraded_context",
        "rebase_fail",
        "ci_failure",
        "merge_fail",
        "retry",
        "captain_decision",
    ] {
        let schema = verdict_json_schema(trigger);
        assert_eq!(schema["type"], "object", "{trigger}");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")), "{trigger}");
        assert!(
            required.contains(&serde_json::json!("feedback")),
            "{trigger}"
        );
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&serde_json::json!("ship")), "{trigger}");
        assert!(actions.contains(&serde_json::json!("nudge")), "{trigger}");
        assert!(actions.contains(&serde_json::json!("respawn")), "{trigger}");
        assert!(
            actions.contains(&serde_json::json!("reset_budget")),
            "{trigger}"
        );
        assert!(
            actions.contains(&serde_json::json!("escalate")),
            "{trigger}"
        );
        assert!(
            !actions.contains(&serde_json::json!("retry_clarifier")),
            "{trigger}"
        );
    }

    // broken_session: only ship, respawn, or escalate. A broken session
    // should never resume in place via nudge/reset_budget.
    let schema = verdict_json_schema("broken_session");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("ship")));
    assert!(actions.contains(&serde_json::json!("respawn")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("nudge")));
    assert!(!actions.contains(&serde_json::json!("reset_budget")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));

    // budget_exhausted routes through the default tier now and still
    // includes escalate + reset_budget.
    let schema = verdict_json_schema("budget_exhausted");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(actions.contains(&serde_json::json!("reset_budget")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));

    // clarifier_fail: only retry_clarifier and escalate.
    let schema = verdict_json_schema("clarifier_fail");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("retry_clarifier")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("ship")));

    // spawn_fail: only respawn and escalate.
    let schema = verdict_json_schema("spawn_fail");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("respawn")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("ship")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));
}

#[test]
fn every_review_trigger_produces_a_usable_schema() {
    // A trigger with an empty action enum would make the model unable to
    // answer at all. `ReviewTrigger::Retry` and `CaptainDecision` reach this
    // code via `unwrap_or(Retry)` in tick_review / dashboard actions.
    for trigger in crate::ALL_REVIEW_TRIGGERS {
        let schema = verdict_json_schema(trigger.as_str());
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(
            !actions.is_empty(),
            "trigger {} has no allowed actions",
            trigger.as_str()
        );
        assert!(
            actions.contains(&serde_json::json!("escalate")),
            "escalate must stay available on every tier ({})",
            trigger.as_str()
        );
    }
}

#[test]
fn schema_offers_high_and_mid_confidence_only() {
    // The prompt, the schema and `validate_verdict` must agree on the same
    // closed set; a `low` anywhere is a three-way disagreement that makes a
    // prompt-following model emit a schema violation on every ship.
    for trigger in crate::ALL_REVIEW_TRIGGERS {
        let schema = verdict_json_schema(trigger.as_str());
        let confidence = schema["properties"]["confidence"]["enum"]
            .as_array()
            .expect("confidence enum present");
        assert_eq!(
            confidence,
            &vec![serde_json::json!("high"), serde_json::json!("mid")],
            "trigger {}",
            trigger.as_str()
        );
    }
}

#[test]
fn schema_carries_confidence_reason_but_never_requires_it_structurally() {
    // Draft-7 cannot express "required only when action = ship", so the
    // requirement lives in the prompt plus `validate_verdict`. Assert the
    // property exists and is not in `required`, so a nudge verdict is not
    // rejected for omitting it.
    let schema = verdict_json_schema("gates_pass");
    assert!(schema["properties"]["confidence_reason"].is_object());
    let required = schema["required"].as_array().unwrap();
    assert!(!required.contains(&serde_json::json!("confidence")));
    assert!(!required.contains(&serde_json::json!("confidence_reason")));
    assert!(!required.contains(&serde_json::json!("report")));
}
