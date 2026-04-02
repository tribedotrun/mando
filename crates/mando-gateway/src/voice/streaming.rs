//! Streaming voice pipeline — runs voice agent in background, sends SSE events
//! through an mpsc channel as they become available.

use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::Event;
use base64::Engine;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::warn;

use crate::voice;
use crate::AppState;

const PROGRESS_PHRASES: &[&str] = &[
    "Still working on it.",
    "Almost there.",
    "Complex request, hang tight.",
];

/// Spawn the voice processing pipeline, sending SSE events through `tx`.
pub(crate) async fn run_voice_pipeline(
    state: AppState,
    text: String,
    session_id_hint: Option<String>,
    tx: mpsc::Sender<Result<Event, Infallible>>,
) {
    // 1. Thinking event.
    send(&tx, "thinking", json!({})).await;

    // 2. Open VoiceDb from the shared pool.
    let db = voice::db::VoiceDb::new(state.db.pool().clone());

    // 3. Create or load session + conversation history (async).
    let session_id = match &session_id_hint {
        Some(id) if db.get_session_exists(id).await.unwrap_or(false) => id.clone(),
        _ => match db.create_session().await {
            Ok(id) => id,
            Err(e) => return send_error_done(&tx, "db", &e.to_string(), None).await,
        },
    };

    let history = db
        .get_messages(&session_id)
        .await
        .inspect_err(|e| {
            tracing::warn!(
                module = "voice",
                session_id = %session_id,
                error = %e,
                "failed to load session history — starting without context"
            );
        })
        .unwrap_or_default()
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    // 4. Immediate audio ack.
    send_ack_audio(&state, &text, &tx).await;

    // 6. Load voice config and template.
    let voice_cfg = state.config.load().voice.clone();
    let voice_wf = state.voice_workflow.load_full();
    let template = match voice_wf.prompts.get("voice_agent").cloned() {
        Some(t) => t,
        None => {
            tracing::error!(
                module = "voice",
                "missing 'voice_agent' prompt in voice workflow config"
            );
            return send_error_done(
                &tx,
                "config",
                "missing 'voice_agent' prompt in voice workflow config",
                None,
            )
            .await;
        }
    };

    // 7. Get daemon URL + auth token for Claude to use.
    let daemon_port = std::fs::read_to_string(mando_config::data_dir().join("daemon.port"))
        .inspect_err(|e| {
            warn!(module = "voice", error = %e, "failed to read daemon.port — falling back to 18600");
        })
        .unwrap_or_else(|_| "18600".into())
        .trim()
        .to_string();
    let daemon_url = format!("http://127.0.0.1:{daemon_port}");
    let auth_token_path = mando_config::data_dir().join("auth-token");
    let auth_token = std::fs::read_to_string(&auth_token_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|e| {
            tracing::warn!(path = %auth_token_path.display(), error = %e, "auth-token file unreadable, voice agent will run unauthenticated");
            String::new()
        });

    // 8. Run voice agent with progress ticker.
    let spoken_response = {
        let text_clone = text.clone();
        let pool = db.pool().clone();
        let mut agent_fut = tokio::spawn(async move {
            voice::intent::run_voice_agent(
                &text_clone,
                &history,
                &template,
                &daemon_url,
                &auth_token,
                &pool,
            )
            .await
        });

        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        ticker.tick().await; // skip immediate tick
        let mut tick_count: usize = 0;

        let agent_result = loop {
            tokio::select! {
                result = &mut agent_fut => { break result; }
                _ = ticker.tick() => {
                    let phrase = PROGRESS_PHRASES[tick_count.min(PROGRESS_PHRASES.len() - 1)];
                    tick_count += 1;
                    send_progress(&state, phrase, &tx).await;
                }
            }
        };

        match agent_result {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => {
                return send_error_done(&tx, "agent", &e.to_string(), Some(&session_id)).await;
            }
            Err(e) => {
                return send_error_done(&tx, "agent", &e.to_string(), Some(&session_id)).await;
            }
        }
    };

    // 9. Text event.
    send(&tx, "text", json!({"chunk": spoken_response})).await;

    // 10. TTS synthesis.
    let tts_start = std::time::Instant::now();
    let tts_error_msg =
        match voice::tts::synthesize(&spoken_response, &voice_cfg.voice_id, &voice_cfg.model).await
        {
            Ok(audio_bytes) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);
                send(&tx, "audio", json!({"bytes": b64})).await;
                None
            }
            Err(e) => {
                let err_msg = e.to_string();
                send(&tx, "error", json!({"source": "tts", "message": &err_msg})).await;
                Some(err_msg)
            }
        };
    let latency_ms = tts_start.elapsed().as_millis() as i64;

    // 11. Log TTS usage and save messages (async).
    let input_chars = spoken_response.chars().count() as i64;
    if let Err(e) = db
        .log_tts_usage(&voice::db_usage::TtsUsageEntry {
            session_id: Some(&session_id),
            input_chars,
            voice_id: &voice_cfg.voice_id,
            model: &voice_cfg.model,
            latency_ms,
            audio_duration_ms: None,
            error: tts_error_msg.as_deref(),
        })
        .await
    {
        tracing::warn!(module = "voice", error = %e, "failed to log TTS usage");
    }
    if let Err(e) = db.add_message(&session_id, "user", &text, None, None).await {
        tracing::warn!(module = "voice", session_id = %session_id, error = %e, "failed to save user message — conversation history gap");
    }
    if let Err(e) = db
        .add_message(&session_id, "assistant", &spoken_response, None, None)
        .await
    {
        tracing::warn!(module = "voice", session_id = %session_id, error = %e, "failed to save assistant message — conversation history gap");
    }

    // 12. Done.
    send(&tx, "done", json!({"session_id": session_id})).await;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

async fn send(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    event_type: &str,
    data: serde_json::Value,
) {
    let event = Event::default().event(event_type).data(data.to_string());
    let _ = tx.send(Ok(event)).await;
}

async fn send_error_done(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    source: &str,
    message: &str,
    session_id: Option<&str>,
) {
    send(tx, "error", json!({"source": source, "message": message})).await;
    send(tx, "done", json!({"session_id": session_id})).await;
}

const ACK_PHRASES: &[&str] = &["On it.", "Let me check.", "Working on it.", "One moment."];

async fn send_ack_audio(
    state: &AppState,
    text: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) {
    let idx = {
        let hash = text.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64));
        (hash as usize) % ACK_PHRASES.len()
    };
    let ack_phrase = ACK_PHRASES[idx];
    let voice_cfg = state.config.load().voice.clone();
    if let Ok(ack_audio) =
        voice::tts::synthesize(ack_phrase, &voice_cfg.voice_id, &voice_cfg.model).await
    {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&ack_audio);
        send(tx, "audio", json!({"bytes": b64})).await;
    }
}

async fn send_progress(
    state: &AppState,
    phrase: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) {
    send(tx, "progress", json!({"message": phrase})).await;

    let voice_cfg = state.config.load().voice.clone();
    match voice::tts::synthesize(phrase, &voice_cfg.voice_id, &voice_cfg.model).await {
        Ok(audio_bytes) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);
            send(tx, "audio", json!({"bytes": b64})).await;
        }
        Err(e) => {
            warn!(module = "voice", error = %e, "progress TTS failed");
        }
    }
}
