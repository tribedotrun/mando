//! AI-powered scout route handlers: research + Q&A.

use std::panic::AssertUnwindSafe;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use futures_util::FutureExt;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{internal_error, not_found_or_internal};
use crate::routes_scout::spawn_scout_processing;
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct ResearchBody {
    pub topic: String,
    pub process: Option<bool>,
}

/// POST /api/scout/research - kick off async research, return run_id immediately.
pub(crate) async fn post_scout_research(
    State(state): State<AppState>,
    Json(body): Json<ResearchBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool().clone();

    // Insert research run row (status=running).
    let run_id = mando_db::queries::scout_research::insert_run(&pool, &body.topic)
        .await
        .map_err(internal_error)?;

    let process = body.process.unwrap_or(true);
    let topic = body.topic.clone();

    // Spawn the actual work as a background task. Clone pool + bus outside
    // the moved closure so the panic handler can still mark the run as
    // failed and notify clients (otherwise the row stays at 'running' and
    // every surface hangs indefinitely).
    let bg_state = state.clone();
    let panic_pool = pool.clone();
    let panic_bus = state.bus.clone();
    state.task_tracker.spawn(async move {
        let result = AssertUnwindSafe(run_research_job(bg_state, run_id, &topic, process))
            .catch_unwind()
            .await;
        if let Err(panic) = result {
            let msg = panic_to_string(&panic);
            tracing::error!(run_id, panic = %msg, "research job panicked");
            if let Err(db_err) =
                mando_db::queries::scout_research::fail_run(&panic_pool, run_id, &msg).await
            {
                tracing::error!(run_id, error = %db_err, "failed to mark panicked run as failed");
            }
            panic_bus.send(
                mando_types::BusEvent::Research,
                Some(json!({"action": "failed", "run_id": run_id, "error": msg})),
            );
        }
    });

    Ok(Json(json!({ "run_id": run_id })))
}

/// Convert a caught panic payload into a human-readable message.
fn panic_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        format!("panic: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic: (unknown payload)".to_string()
    }
}

/// Background research job.
async fn run_research_job(state: AppState, run_id: i64, topic: &str, process: bool) {
    let pool = state.db.pool().clone();
    let bus = &state.bus;
    let workflow = state.scout_workflow.load_full();

    // Emit research_started SSE.
    bus.send(
        mando_types::BusEvent::Research,
        Some(json!({"action": "started", "run_id": run_id, "research_prompt": topic})),
    );

    // Spawn heartbeat emitter.
    let heartbeat_cancel = tokio_util::sync::CancellationToken::new();
    let hb_cancel = heartbeat_cancel.clone();
    let hb_bus = state.bus.clone();
    let hb_handle = tokio::spawn(async move {
        // Exponential heartbeat fire times: 2m, 4m, 8m, 16m elapsed.
        // These are gaps between fires, not absolute times, so each entry
        // is the delta from the previous heartbeat.
        let gaps = [120u64, 120, 240, 480];
        let mut elapsed = 0u64;
        for wait in gaps {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {
                    elapsed += wait;
                    hb_bus.send(
                        mando_types::BusEvent::Research,
                        Some(json!({"action": "progress", "run_id": run_id, "elapsed_s": elapsed})),
                    );
                }
                _ = hb_cancel.cancelled() => return,
            }
        }
    });

    // Run the CC session.
    let research_result = mando_scout::runtime::research::run_research(topic, &workflow).await;
    heartbeat_cancel.cancel();
    let _ = hb_handle.await;

    match research_result {
        Ok(output) => {
            // Record the research session.
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
                tracing::warn!(error = %e, "failed to record research session");
            }
            state.bus.send(mando_types::BusEvent::Sessions, None);

            // Process discovered links (capped by research_max_items).
            let max_items = workflow.agent.research_max_items;
            let mut added_count = 0i64;
            let mut errors: Vec<Value> = Vec::new();
            let mut links_json: Vec<Value> = Vec::new();

            for link in output.result.links.iter().take(max_items) {
                match mando_scout::add_scout_item(&pool, &link.url, Some(&link.title)).await {
                    Ok(val) => {
                        let id = val["id"].as_i64();
                        let was_added = val["added"].as_bool() == Some(true);

                        if let Some(id) = id {
                            if was_added {
                                // New item: stamp with this run's FK to
                                // record who discovered it. Existing items
                                // keep their original research_run_id so
                                // historical attribution is preserved.
                                if let Err(e) =
                                    mando_db::queries::scout::set_research_run_id(&pool, id, run_id)
                                        .await
                                {
                                    tracing::warn!(scout_id = id, error = %e, "failed to set research_run_id");
                                }
                                added_count += 1;
                                let scout_payload =
                                    mando_scout::get_scout_item(&pool, id).await.ok();
                                bus.send(
                                    mando_types::BusEvent::Scout,
                                    Some(json!({"action": "created", "item": scout_payload, "id": id})),
                                );
                                if process {
                                    spawn_scout_processing(&state, id, link.url.clone());
                                }
                            } else {
                                // Existing item: retry if it was stuck at
                                // error. Don't touch research_run_id.
                                let current_status = mando_scout::get_scout_item(&pool, id)
                                    .await
                                    .ok()
                                    .and_then(|v| v["status"].as_str().map(str::to_string));
                                if current_status.as_deref() == Some("error") && process {
                                    match mando_db::queries::scout::reset_error_state(&pool, id)
                                        .await
                                    {
                                        Err(e) => {
                                            tracing::warn!(scout_id = id, error = %e, "failed to reset error state")
                                        }
                                        Ok(()) => {
                                            spawn_scout_processing(&state, id, link.url.clone());
                                            // Emit `updated` so the renderer
                                            // reflects the status flip from
                                            // error → pending.
                                            let scout_payload =
                                                mando_scout::get_scout_item(&pool, id).await.ok();
                                            bus.send(
                                                mando_types::BusEvent::Scout,
                                                Some(json!({"action": "updated", "item": scout_payload, "id": id})),
                                            );
                                        }
                                    }
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
                            "error": e.to_string(),
                        }));
                    }
                }
            }

            // Complete the research run.
            if let Err(e) = mando_db::queries::scout_research::complete_run(
                &pool,
                run_id,
                &output.session_id,
                added_count,
            )
            .await
            {
                tracing::warn!(run_id, error = %e, "failed to complete research run");
            }

            bus.send(
                mando_types::BusEvent::Research,
                Some(json!({
                    "action": "completed",
                    "run_id": run_id,
                    "links": links_json,
                    "added_count": added_count,
                    "errors": errors,
                })),
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            if let Err(db_err) =
                mando_db::queries::scout_research::fail_run(&pool, run_id, &error_msg).await
            {
                tracing::warn!(run_id, error = %db_err, "failed to mark research run as failed");
            }
            bus.send(
                mando_types::BusEvent::Research,
                Some(json!({"action": "failed", "run_id": run_id, "error": error_msg})),
            );
        }
    }
}

/// GET /api/scout/research/{id} - poll research run status.
pub(crate) async fn get_scout_research_run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let run = mando_db::queries::scout_research::get_run(pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found_or_internal(anyhow::anyhow!("research run #{id} not found")))?;
    Ok(Json(serde_json::to_value(&run).map_err(internal_error)?))
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
