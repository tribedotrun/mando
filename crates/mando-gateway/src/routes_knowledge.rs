//! /api/knowledge/* and /api/self-improve/* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use mando_captain::runtime::guardian::SelfImproveGuardian;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Gate: return 503 if the decision journal feature is disabled.
fn require_journal(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    if !state.config.load().features.decision_journal {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "decision journal is disabled",
        ));
    }
    Ok(())
}

/// GET /api/knowledge
pub(crate) async fn get_knowledge(State(_state): State<AppState>) -> Json<Value> {
    let knowledge_path = mando_config::state_dir()
        .join("knowledge")
        .join("approved.json");
    let approved: Vec<Value> = match std::fs::read_to_string(&knowledge_path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(
                module = "knowledge",
                path = %knowledge_path.display(),
                error = %e,
                "approved knowledge file corrupt — returning empty"
            );
            Vec::new()
        }),
        Err(e) => {
            tracing::warn!(module = "knowledge", path = %knowledge_path.display(), error = %e, "cannot read approved knowledge file");
            Vec::new()
        }
    };
    let count = approved.len();
    Json(json!({
        "approved": approved,
        "count": count,
    }))
}

/// GET /api/knowledge/pending — scan knowledge dir for individual lesson files with status=pending.
pub(crate) async fn get_knowledge_pending(State(_state): State<AppState>) -> Json<Value> {
    let knowledge_dir = mando_config::state_dir().join("knowledge");
    let mut pending: Vec<Value> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&knowledge_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip approved.json and non-json files.
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("approved.json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if val.get("status").and_then(|s| s.as_str()) == Some("pending") {
                        pending.push(val);
                    }
                }
            }
        }
    }

    let count = pending.len();
    Json(json!({
        "pending": pending,
        "count": count,
    }))
}

#[derive(Deserialize)]
pub(crate) struct ApproveLessonsBody {
    pub lessons: Vec<Value>,
}

/// POST /api/knowledge/approve
///
/// Each lesson in the body must have an `"id"` field.  For every lesson we:
///   1. Call `distiller::approve_knowledge(id)` which reads the individual
///      `{id}.json` file, sets its status to "approved", and writes it back.
///   2. Append the full lesson object to `approved.json` for quick lookup.
pub(crate) async fn post_knowledge_approve(
    State(_state): State<AppState>,
    Json(body): Json<ApproveLessonsBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let knowledge_dir = mando_config::state_dir().join("knowledge");
    let approved_path = knowledge_dir.join("approved.json");

    // Ensure directory exists.
    if let Err(e) = std::fs::create_dir_all(&knowledge_dir) {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("cannot create knowledge dir: {e}"),
        ));
    }

    let mut approved_count = 0usize;

    for lesson in &body.lessons {
        let id = lesson
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        // Update the individual lesson JSON file (status -> "approved").
        if let Err(e) = mando_captain::runtime::distiller::approve_knowledge(id).await {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("approve lesson '{id}': {e}"),
            ));
        }
        approved_count += 1;
    }

    // Also append to approved.json for aggregate access.
    let mut approved: Vec<Value> = match std::fs::read_to_string(&approved_path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(
                module = "knowledge",
                path = %approved_path.display(),
                error = %e,
                "approved knowledge file corrupt — starting fresh"
            );
            Vec::new()
        }),
        Err(e) => {
            tracing::warn!(module = "knowledge", path = %approved_path.display(), error = %e, "cannot read approved knowledge file");
            Vec::new()
        }
    };
    approved.extend(body.lessons.iter().cloned());

    let json = match serde_json::to_string_pretty(&approved) {
        Ok(j) => j,
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to serialize lessons: {e}"),
            ));
        }
    };
    if let Err(e) = std::fs::write(&approved_path, json) {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        ));
    }

    Ok(Json(json!({
        "ok": true,
        "added": approved_count,
        "total": approved.len(),
    })))
}

/// POST /api/knowledge/learn — now runs the pattern distiller.
pub(crate) async fn post_knowledge_learn(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_journal(&state)?;

    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let pool = state.db.pool();
    match mando_captain::runtime::distiller::run_distiller(&config, &workflow, pool).await {
        Ok(result) => {
            // Notify TG for high-confidence patterns.
            notify_patterns(&state.bus, &result.patterns);
            Ok(Json(json!({
                "ok": true,
                "summary": result.summary,
                "patterns_found": result.patterns_found,
                "patterns": result.patterns,
            })))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// Send TG notifications for high-confidence patterns via EventBus.
pub(crate) fn notify_patterns(
    bus: &mando_shared::EventBus,
    patterns: &[mando_captain::io::journal_types::Pattern],
) {
    use mando_types::events::{NotificationKind, NotificationPayload};
    use mando_types::notify::NotifyLevel;
    use mando_types::BusEvent;

    for p in patterns {
        if p.confidence < 0.7 {
            continue;
        }
        let msg = format!(
            "Pattern detected (confidence {:.0}%):\n{}\n\nRecommendation: {}",
            p.confidence * 100.0,
            p.pattern,
            p.recommendation,
        );
        let payload = NotificationPayload {
            message: msg,
            level: NotifyLevel::Normal,
            kind: NotificationKind::Generic,
            task_key: None,
            reply_markup: None,
        };
        bus.send(
            BusEvent::Notification,
            Some(serde_json::to_value(&payload).unwrap_or_default()),
        );
    }
}

/// GET /api/journal?worker=X&limit=N — query recent decisions.
pub(crate) async fn get_journal(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_journal(&state)?;

    let jdb = mando_captain::io::journal::JournalDb::new(state.db.pool().clone());

    let worker = params.get("worker").map(|s| s.as_str());
    let limit: usize = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let decisions = jdb.recent_decisions(worker, limit).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("query failed: {e}"),
        )
    })?;

    let (total, successes, failures, unresolved) = jdb.total_counts().await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("totals query failed: {e}"),
        )
    })?;

    Ok(Json(json!({
        "decisions": decisions,
        "count": decisions.len(),
        "totals": {
            "total": total,
            "successes": successes,
            "failures": failures,
            "unresolved": unresolved,
        },
    })))
}

/// GET /api/patterns?status=X — list patterns.
pub(crate) async fn get_patterns(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_journal(&state)?;

    let jdb = mando_captain::io::journal::JournalDb::new(state.db.pool().clone());

    let status = params.get("status").map(|s| s.as_str());
    let patterns = jdb.list_patterns(status).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("query failed: {e}"),
        )
    })?;

    Ok(Json(json!({
        "patterns": patterns,
        "count": patterns.len(),
    })))
}

#[derive(Deserialize)]
pub(crate) struct PatternActionBody {
    pub id: i64,
    pub status: String,
}

/// POST /api/patterns/update — approve/dismiss a pattern.
pub(crate) async fn post_pattern_update(
    State(state): State<AppState>,
    Json(body): Json<PatternActionBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_journal(&state)?;

    if body.status != "approved" && body.status != "dismissed" {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "status must be 'approved' or 'dismissed'",
        ));
    }
    let jdb = mando_captain::io::journal::JournalDb::new(state.db.pool().clone());
    jdb.update_pattern_status(body.id, &body.status)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("update failed: {e}"),
            )
        })?;
    Ok(Json(
        json!({"ok": true, "id": body.id, "status": body.status}),
    ))
}

#[derive(Deserialize)]
pub(crate) struct SelfImproveBody {
    pub text: String,
}

/// POST /api/self-improve/trigger
pub(crate) async fn post_self_improve_trigger(
    State(state): State<AppState>,
    Json(body): Json<SelfImproveBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.config.load().features.dev_mode {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "dev mode is disabled",
        ));
    }

    let config = (*state.config.load_full()).clone();
    let workflow = (*state.captain_workflow.load_full()).clone();
    let pool = state.db.pool().clone();
    let mut guardian = SelfImproveGuardian::new(config, workflow, pool);

    let text = if body.text.is_empty() {
        None
    } else {
        Some(body.text.as_str())
    };

    let result = guardian.trigger_once(text).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("guardian error: {e}"),
        )
    })?;

    Ok(Json(json!({
        "ok": true,
        "triggered": result.triggered,
        "skippedReason": result.skipped_reason,
        "incidents": result.incidents,
        "repairOutput": result.repair_output,
    })))
}
