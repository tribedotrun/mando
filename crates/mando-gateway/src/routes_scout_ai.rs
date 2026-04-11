//! AI-powered scout route handlers: research + Q&A.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::internal_error;
use crate::routes_scout::spawn_scout_processing;
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct ResearchBody {
    pub topic: String,
    pub process: Option<bool>,
}

/// POST /api/scout/research
pub(crate) async fn post_scout_research(
    State(state): State<AppState>,
    Json(body): Json<ResearchBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let workflow = state.scout_workflow.load_full();
    let pool = state.db.pool().clone();
    let process = body.process.unwrap_or(false);

    let output = mando_scout::runtime::research::run_research(&body.topic, &workflow)
        .await
        .map_err(internal_error)?;

    // Record the research session in cc_sessions (topic-level, no scout_item_id).
    let scout_db = mando_scout::ScoutDb::new(pool.clone());
    if let Err(e) = scout_db
        .record_session(
            None,
            &output.session_id,
            "scout-research",
            output.cost_usd,
            output.duration_ms,
        )
        .await
    {
        tracing::warn!(error = %e, "post_scout_research: failed to record session");
    }
    state.bus.send(mando_types::BusEvent::Sessions, None);

    let mut added_count = 0u64;
    let mut errors: Vec<Value> = Vec::new();
    let mut links_json: Vec<Value> = Vec::new();

    for link in &output.result.links {
        match mando_scout::add_scout_item(&pool, &link.url, Some(&link.title)).await {
            Ok(val) => {
                let id = val["id"].as_i64();
                let was_added = val["added"].as_bool() == Some(true);

                if was_added {
                    added_count += 1;
                    if let Some(id) = id {
                        let scout_payload = mando_scout::get_scout_item(&pool, id).await.ok();
                        state.bus.send(
                            mando_types::BusEvent::Scout,
                            Some(json!({"action": "created", "item": scout_payload, "id": id})),
                        );
                        if process {
                            spawn_scout_processing(&state, id, link.url.clone());
                        }
                    }
                }

                links_json.push(json!({
                    "url": link.url,
                    "title": link.title,
                    "type": link.link_type,
                    "reason": link.reason,
                    "id": id,
                    "added": was_added,
                }));
            }
            Err(e) => {
                errors.push(json!({
                    "url": link.url,
                    "stage": "add",
                    "error": e.to_string(),
                }));
                links_json.push(json!({
                    "url": link.url,
                    "title": link.title,
                    "type": link.link_type,
                    "reason": link.reason,
                    "id": null,
                    "added": false,
                }));
            }
        }
    }

    Ok(Json(json!({
        "ok": true,
        "links": links_json,
        "added": added_count,
        "processing": added_count > 0 && process,
        "errors": errors,
    })))
}

#[derive(Deserialize)]
pub(crate) struct ScoutAskBody {
    pub id: i64,
    pub question: String,
    /// Pass back from previous response to resume the same CC session.
    pub session_id: Option<String>,
}

/// POST /api/scout/ask
pub(crate) async fn post_scout_ask(
    State(state): State<AppState>,
    Json(body): Json<ScoutAskBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let workflow = state.scout_workflow.load_full();
    let qa_mgr = state.qa_session_mgr.clone();
    let pool = state.db.pool();
    let id = body.id;
    let question = body.question;
    let session_key = body.session_id;

    let item = mando_scout::get_scout_item(pool, id)
        .await
        .map_err(internal_error)?;
    let article_data = mando_scout::ensure_scout_article(pool, id, &workflow)
        .await
        .map_err(internal_error)?;

    let summary = item["summary"]
        .as_str()
        .unwrap_or("(no summary)")
        .to_string();
    let article = article_data["article"]
        .as_str()
        .unwrap_or("(no article content)")
        .to_string();

    let raw_path = mando_scout::content_path(id);
    let raw_note = if raw_path.exists() {
        Some(format!(
            "The original source content is saved at `{}`. Read it for full detail.",
            raw_path.display()
        ))
    } else {
        None
    };

    let qa_result = qa_mgr
        .ask(
            &question,
            &summary,
            &article,
            raw_note.as_deref(),
            &workflow,
            session_key.as_deref(),
        )
        .await
        .map_err(internal_error)?;

    // Record the Q&A session in cc_sessions.
    if let Some(ref sid) = qa_result.session_id {
        let scout_db = mando_scout::ScoutDb::new(pool.clone());
        if let Err(e) = scout_db
            .record_session(
                Some(id),
                sid,
                "scout-qa",
                qa_result.cost_usd,
                qa_result.duration_ms,
            )
            .await
        {
            tracing::warn!(error = %e, "post_scout_ask: failed to record session");
        }
        state.bus.send(mando_types::BusEvent::Sessions, None);
    }

    Ok(Json(json!({
        "ok": true,
        "id": id,
        "answer": qa_result.answer,
        "session_id": qa_result.session_id,
        "suggested_followups": qa_result.suggested_followups,
        "session_reset": qa_result.session_reset,
    })))
}
