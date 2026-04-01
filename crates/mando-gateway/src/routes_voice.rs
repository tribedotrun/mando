//! /api/voice/* route handlers — voice control SSE endpoint and metadata queries.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Multipart, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::response::{error_response, internal_error};
use crate::voice;
use crate::AppState;

/// Gate: return 503 if the voice feature is disabled.
fn require_voice(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    if !state.config.load().features.voice {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "voice is disabled",
        ));
    }
    Ok(())
}

// ── POST /api/voice ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct VoiceBody {
    pub text: String,
    pub session_id: Option<String>,
}

/// POST /api/voice — process a voice command, returning a real-time SSE stream.
pub(crate) async fn post_voice(
    State(state): State<AppState>,
    Json(body): Json<VoiceBody>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    require_voice(&state)?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(voice::streaming::run_voice_pipeline(
        state,
        body.text,
        body.session_id,
        tx,
    ));

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("heartbeat"),
    ))
}

// ── GET /api/voice/usage ────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(crate) struct UsageQuery {
    pub days: Option<u32>,
    #[serde(default)]
    pub detail: bool,
}

/// GET /api/voice/usage — TTS usage summary and optionally detailed records.
pub(crate) async fn get_voice_usage(
    State(state): State<AppState>,
    Query(params): Query<UsageQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_voice(&state)?;

    let db = voice::db::VoiceDb::new(state.db.pool().clone());
    let days = params.days.unwrap_or(30);
    let summary = db.get_usage_summary(days).await.map_err(internal_error)?;
    let mut val = serde_json::to_value(&summary).unwrap_or(json!({}));
    if params.detail {
        let records = db
            .get_usage_detail(100, days)
            .await
            .map_err(internal_error)?;
        val["records"] = serde_json::to_value(&records).unwrap_or(json!([]));
    }
    Ok(Json(val))
}

// ── GET /api/voice/sessions/:id/messages ────────────────────────────────────

/// GET /api/voice/sessions/:id/messages — list messages in a voice session.
pub(crate) async fn get_voice_messages(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_voice(&state)?;

    let db = voice::db::VoiceDb::new(state.db.pool().clone());
    let messages = db.get_messages(&session_id).await.map_err(internal_error)?;
    Ok(Json(
        json!({ "messages": serde_json::to_value(&messages).unwrap_or(json!([])) }),
    ))
}

// ── GET /api/voice/sessions ─────────────────────────────────────────────────

/// GET /api/voice/sessions — list voice sessions (pruning expired ones first).
pub(crate) async fn get_voice_sessions(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_voice(&state)?;

    let db = voice::db::VoiceDb::new(state.db.pool().clone());
    let expiry_hours = state.config.load().voice.session_expiry_days as u64 * 24;

    let pruned = db
        .prune_expired(expiry_hours)
        .await
        .map_err(internal_error)?;
    if pruned > 0 {
        tracing::debug!(
            module = "voice",
            pruned = pruned,
            "pruned expired voice sessions"
        );
    }

    // Voice sessions: only include sessions that have actual voice messages,
    // excluding internal CC one-shot sessions logged with caller="voice-agent".
    let pool = state.db.pool();
    let sessions = mando_db::queries::voice::list_conversation_sessions(pool, 50)
        .await
        .map_err(internal_error)?;

    // Enrich with voice-specific title (first user message).
    let mut session_values = Vec::with_capacity(sessions.len());
    for s in &sessions {
        let title = mando_db::queries::voice::voice_session_title(pool, &s.session_id)
            .await
            .ok()
            .flatten();
        let mut v = serde_json::to_value(s).unwrap_or(json!({}));
        if let Value::Object(ref mut map) = v {
            map.insert(
                "title".into(),
                title.map(Value::String).unwrap_or(Value::Null),
            );
        }
        session_values.push(v);
    }

    Ok(Json(json!({
        "sessions": session_values,
        "pruned": pruned,
    })))
}

// ── POST /api/voice/transcribe ───────────────────────────────────────────────

/// POST /api/voice/transcribe — transcribe uploaded audio via ElevenLabs Scribe.
pub(crate) async fn post_voice_transcribe(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_voice(&state)?;

    let mut audio_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let data = field
                .bytes()
                .await
                .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
            audio_bytes = Some(data.to_vec());
        }
    }

    let bytes = audio_bytes
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "missing 'file' field"))?;

    match voice::stt::transcribe(&bytes).await {
        Ok(text) => Ok(Json(json!({"text": text}))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}
