//! Extracted helpers for the deterministic classifier.

use std::collections::HashMap;
use std::path::Path;

use crate::{Action, ActionKind, WorkerContext};
use anyhow::Result;
use global_claude::{BrokenSessionMatch, StreamSymptomMatcher};
use rustc_hash::FxHashMap;

/// Check if stream output contains substantive content (20+ non-whitespace chars).
/// Used as a quality gate for no-PR task completion. Works against the
/// synthesized `ctx.stream_tail` (`extract_stream_tail`) because it cares
/// about output density, not event structure.
pub(super) fn has_substantive_output(stream_tail: &str) -> bool {
    let non_ws_count = stream_tail.chars().filter(|c| !c.is_whitespace()).count();
    non_ws_count >= 20
}

/// Check if the worker is stuck on a CC image-dimension-limit error.
///
/// Structural: walks the stream file events and inspects the last
/// `user/tool_result/is_error:true` content against the `ImageDimensionLimit`
/// rule. Returns `false` when the stream path is missing — image-dimension
/// nudges are recoverable and the classifier has other signals to route on.
pub(super) fn is_image_dimension_blocked(
    stream_path: Option<&Path>,
    symptoms: &StreamSymptomMatcher,
) -> bool {
    let Some(path) = stream_path else {
        return false;
    };
    global_claude::detect_image_dimension_blocked(path, symptoms)
}

/// Detect a broken-session CC signal in the worker's stream file.
///
/// Returns a typed match when the structural detector fires — either a
/// CC-reported abort (terminal `result/is_error:true`) or an externally-
/// killed session (`SessionInterrupted`). The recoverable `ImageDimensionLimit`
/// symptom stays on the nudge path and is never returned here.
///
/// Returns `None` when no stream path is available (the classifier's
/// fallback for workers that never had a CC session). Other classifier
/// rules (budget, timeout, gates) still apply in that case.
pub(super) fn detect_broken_session_symptom(
    stream_path: Option<&Path>,
    symptoms: &StreamSymptomMatcher,
) -> Option<BrokenSessionMatch> {
    let Some(path) = stream_path else {
        tracing::trace!(
            module = "captain-deterministic",
            "broken-session detector skipped — no stream path for this worker"
        );
        return None;
    };
    global_claude::stream_broken_session_symptom(path, symptoms)
}

/// Build an Action from worker context.
pub(super) fn action(ctx: &WorkerContext, kind: ActionKind, message: &str, reason: &str) -> Action {
    Action {
        worker: ctx.session_name.clone(),
        action: kind,
        message: if message.is_empty() {
            None
        } else {
            Some(message.to_string())
        },
        reason: if reason.is_empty() {
            None
        } else {
            Some(reason.to_string())
        },
    }
}

pub(super) fn classify_unresolved_threads(
    ctx: &WorkerContext,
    nudges: &HashMap<String, String>,
) -> Result<Action> {
    let mut parts = Vec::new();
    if ctx.unresolved_threads > 0 {
        parts.push(format!(
            "{} unresolved review thread(s)",
            ctx.unresolved_threads
        ));
    }
    if ctx.unreplied_threads > 0 {
        parts.push(format!(
            "{} unreplied review thread(s)",
            ctx.unreplied_threads
        ));
    }
    if ctx.unaddressed_issue_comments > 0 {
        parts.push(format!(
            "{} unaddressed issue comment(s)",
            ctx.unaddressed_issue_comments
        ));
    }
    let detail = parts.join(" and ");
    let pr = ctx.pr.as_deref().unwrap_or("");
    let pr_num = pr
        .trim_start_matches('#')
        .trim_start_matches(|c: char| !c.is_ascii_digit());
    let pr_num = pr_num
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("pr", pr);
    vars.insert("detail", detail.as_str());
    vars.insert("pr_num", pr_num);
    let msg = render_nudge(nudges, "unresolved_threads", &vars)?;
    Ok(action(
        ctx,
        ActionKind::Nudge,
        &msg,
        &format!("PR has {}", detail),
    ))
}

/// Render a nudge from the workflow template. Returns an error with the
/// template key in context so the tick can escalate when a template is
/// missing or malformed. Callers must not panic on template errors.
pub(super) fn render_nudge(
    nudges: &HashMap<String, String>,
    key: &str,
    vars: &FxHashMap<&str, &str>,
) -> Result<String> {
    settings::render_nudge(key, nudges, vars).map_err(|e| {
        anyhow::anyhow!("nudge template '{key}' missing from captain-workflow.yaml: {e}")
    })
}
