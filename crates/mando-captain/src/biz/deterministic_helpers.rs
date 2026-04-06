//! Extracted helpers for the deterministic classifier.

use std::collections::HashMap;

use anyhow::Result;
use mando_types::captain::{Action, ActionKind};
use mando_types::WorkerContext;
use rustc_hash::FxHashMap;

/// Check if stream output contains substantive content (20+ non-whitespace chars).
/// Used as a quality gate for no-PR task completion.
pub(super) fn has_substantive_output(stream_tail: &str) -> bool {
    let non_ws_count = stream_tail.chars().filter(|c| !c.is_whitespace()).count();
    non_ws_count >= 20
}

/// Check if worker is stuck due to CC image dimension limit error.
pub(super) fn is_image_dimension_blocked(stream_tail: &str) -> bool {
    stream_tail.contains("exceeds the dimension limit") && stream_tail.contains("2000px")
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
    mando_config::render_nudge(key, nudges, vars).map_err(|e| {
        anyhow::anyhow!("nudge template '{key}' missing from captain-workflow.yaml: {e}")
    })
}
