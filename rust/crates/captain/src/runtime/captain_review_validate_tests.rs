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
            report: Some("Tried respawn 3 times, each run wedged on advisor".into()),
            ..Default::default()
        };
        let result = validate_verdict(verdict.clone(), &item);
        assert_eq!(result.action, "escalate", "trigger={}", trigger.as_str());
        assert_eq!(result.feedback, "beyond recovery");
        assert_eq!(
            result.report.as_deref(),
            Some("Tried respawn 3 times, each run wedged on advisor"),
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
        feedback: "wedged on advisor".into(),
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
        report.contains("wedged on advisor"),
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
