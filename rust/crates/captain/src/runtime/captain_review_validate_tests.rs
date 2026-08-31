//! `validate_verdict` coverage — invalid actions get coerced to `escalate`,
//! and `escalate` without a report gets a synthesized placeholder so the
//! human has something to triage instead of a bare status.

use super::*;

#[test]
fn test_validate_verdict_rejects_invalid_action() {
    let item = Task {
        captain_review_trigger: Some(crate::ReviewTrigger::GatesPass),
        ..Task::new("test")
    };
    let verdict = CaptainVerdict {
        action: "approve".into(),
        feedback: "looks good".into(),
        ..Default::default()
    };
    let result = validate_verdict(verdict, &item);
    assert_eq!(result.action, "escalate");
    assert!(result.feedback.contains("approve"));
}

#[test]
fn test_validate_verdict_accepts_escalate_with_non_empty_report() {
    for trigger in [
        crate::ReviewTrigger::GatesPass,
        crate::ReviewTrigger::BrokenSession,
        crate::ReviewTrigger::RepeatedNudge,
        crate::ReviewTrigger::Timeout,
        crate::ReviewTrigger::BudgetExhausted,
    ] {
        let item = Task {
            captain_review_trigger: Some(trigger),
            ..Task::new("test")
        };
        let verdict = CaptainVerdict {
            action: "escalate".into(),
            feedback: "beyond recovery".into(),
            report: Some("Tried respawn 3 times, each run wedged on startup".into()),
            ..Default::default()
        };
        let result = validate_verdict(verdict.clone(), &item);
        assert_eq!(result.action, "escalate", "trigger={}", trigger.as_str());
        assert_eq!(result.feedback, "beyond recovery");
        assert_eq!(
            result.report.as_deref(),
            Some("Tried respawn 3 times, each run wedged on startup"),
            "non-empty report must pass through unchanged",
        );
    }
}

#[test]
fn test_validate_verdict_synthesizes_report_when_escalate_has_none() {
    // Escalate without a report must still return escalate (we don't want to
    // silently demote to nudge and loop again) but must synthesize a
    // placeholder report so the human gets context instead of a bare status.
    let item = Task {
        captain_review_trigger: Some(crate::ReviewTrigger::BrokenSession),
        ..Task::new("test")
    };
    let verdict = CaptainVerdict {
        action: "escalate".into(),
        feedback: "wedged on startup".into(),
        report: None,
        ..Default::default()
    };
    let result = validate_verdict(verdict, &item);
    assert_eq!(result.action, "escalate");
    let report = result.report.expect("report must be synthesized");
    assert!(
        report.contains("broken_session"),
        "report should name the trigger: {report}",
    );
    assert!(
        report.contains("wedged on startup"),
        "report should preserve the feedback text: {report}",
    );
    assert!(
        report.contains("Manual triage required"),
        "report should tell the human what to do: {report}",
    );
}

#[test]
fn test_validate_verdict_rejects_broken_session_nudge() {
    let item = Task {
        captain_review_trigger: Some(crate::ReviewTrigger::BrokenSession),
        ..Task::new("test")
    };
    let verdict = CaptainVerdict {
        action: "nudge".into(),
        feedback: "try resuming the same session".into(),
        ..Default::default()
    };
    let result = validate_verdict(verdict, &item);
    assert_eq!(result.action, "escalate");
    assert!(result.feedback.contains("nudge"));
    assert!(result.feedback.contains("broken_session"));
}

// ── Confidence normalization (high / mid only) ──

fn ship(confidence: Option<&str>) -> CaptainVerdict {
    CaptainVerdict {
        action: "ship".into(),
        feedback: "work is done".into(),
        confidence: confidence.map(str::to_string),
        confidence_reason: Some("deck 12-0.png plus the diff hunk in feed.rs".into()),
        ..Default::default()
    }
}

fn ship_item(trigger: crate::ReviewTrigger) -> Task {
    Task {
        captain_review_trigger: Some(trigger),
        ..Task::new("test")
    }
}

#[test]
fn valid_confidences_pass_through_untouched() {
    for grade in ["high", "mid"] {
        let item = ship_item(crate::ReviewTrigger::GatesPass);
        let result = validate_verdict(ship(Some(grade)), &item);
        assert_eq!(result.action, "ship");
        assert_eq!(result.confidence.as_deref(), Some(grade));
        assert_eq!(
            result.confidence_reason.as_deref(),
            Some("deck 12-0.png plus the diff hunk in feed.rs"),
        );
    }
}

#[test]
fn low_confidence_is_no_longer_accepted_and_coerces_to_mid() {
    // The enforced schema offers `high` / `mid`; the validator used to accept
    // a third value the schema rejects. Anything outside the closed set now
    // lands on `mid`, which ships to AwaitingReview without auto-merging.
    let item = ship_item(crate::ReviewTrigger::GatesPass);
    let result = validate_verdict(ship(Some("low")), &item);
    assert_eq!(result.confidence.as_deref(), Some("mid"));
}

#[test]
fn budget_exhausted_ship_no_longer_defaults_to_low() {
    // The old code reserved `low` for forced ships under budget_exhausted.
    // With `low` gone, that path must produce `mid` like every other trigger.
    let item = ship_item(crate::ReviewTrigger::BudgetExhausted);
    let result = validate_verdict(ship(None), &item);
    assert_eq!(result.confidence.as_deref(), Some("mid"));
}

#[test]
fn missing_or_unknown_confidence_coerces_to_mid() {
    for supplied in [
        None,
        Some(""),
        Some("HIGH"),
        Some("very high"),
        Some("none"),
    ] {
        let item = ship_item(crate::ReviewTrigger::Timeout);
        let result = validate_verdict(ship(supplied), &item);
        assert_eq!(
            result.confidence.as_deref(),
            Some("mid"),
            "confidence {supplied:?} must coerce to mid, never auto-merge"
        );
    }
}

#[test]
fn missing_confidence_reason_gets_a_visible_placeholder() {
    let item = ship_item(crate::ReviewTrigger::GatesPass);
    for blank in [None, Some(""), Some("   ")] {
        let verdict = CaptainVerdict {
            confidence_reason: blank.map(str::to_string),
            ..ship(Some("high"))
        };
        let result = validate_verdict(verdict, &item);
        assert!(result
            .confidence_reason
            .as_deref()
            .unwrap()
            .contains("check evidence manually"));
    }
}

#[test]
fn non_ship_verdicts_never_carry_confidence() {
    for action in ["nudge", "respawn", "reset_budget"] {
        let item = ship_item(crate::ReviewTrigger::GatesPass);
        let verdict = CaptainVerdict {
            action: action.into(),
            confidence: Some("high".into()),
            confidence_reason: Some("should be stripped".into()),
            ..ship(Some("high"))
        };
        let result = validate_verdict(verdict, &item);
        assert_eq!(result.action, action);
        assert_eq!(
            result.confidence, None,
            "{action} must not carry confidence"
        );
        assert_eq!(result.confidence_reason, None, "{action}");
    }
}

#[test]
fn broken_session_ship_still_grades_confidence() {
    // broken_session may ship (the work was already done before the wedge),
    // so the confidence path must apply on that tier too.
    let item = ship_item(crate::ReviewTrigger::BrokenSession);
    let result = validate_verdict(ship(Some("high")), &item);
    assert_eq!(result.action, "ship");
    assert_eq!(result.confidence.as_deref(), Some("high"));
}

#[test]
fn test_validate_verdict_synthesizes_report_when_escalate_report_is_blank() {
    // Empty string and whitespace-only reports are treated the same as None.
    let item = Task {
        captain_review_trigger: Some(crate::ReviewTrigger::RepeatedNudge),
        ..Task::new("test")
    };
    for blank in ["", "   ", "\n\t\n"] {
        let verdict = CaptainVerdict {
            action: "escalate".into(),
            feedback: "".into(),
            report: Some(blank.into()),
            ..Default::default()
        };
        let result = validate_verdict(verdict, &item);
        assert_eq!(result.action, "escalate", "blank={blank:?}");
        let report = result.report.expect("report must be synthesized");
        assert!(
            !report.trim().is_empty(),
            "synthesized report must be non-empty (blank={blank:?})"
        );
        assert!(
            report.contains("Manual triage required"),
            "report should tell the human what to do: {report}",
        );
    }
}
