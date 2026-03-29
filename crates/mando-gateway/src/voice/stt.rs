//! ElevenLabs Scribe STT — speech-to-text transcription via ElevenLabs API.
//!
//! Uses the same `ELEVENLABS_API_KEY` as TTS.

use anyhow::{bail, Context, Result};

const SCRIBE_URL: &str = "https://api.elevenlabs.io/v1/speech-to-text";

/// Transcribe audio bytes to text using ElevenLabs Scribe v2.
pub(crate) async fn transcribe(audio_bytes: &[u8]) -> Result<String> {
    let key = std::env::var("ELEVENLABS_API_KEY")
        .context("ELEVENLABS_API_KEY not set in config.json env section")?;

    let part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
        .file_name("audio.webm")
        .mime_str("audio/webm")
        .context("invalid mime type")?;

    let form = reqwest::multipart::Form::new()
        .text("model_id", "scribe_v2")
        .part("file", part);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("build HTTP client")?;

    let resp = client
        .post(SCRIBE_URL)
        .header("xi-api-key", &key)
        .multipart(form)
        .send()
        .await
        .context("ElevenLabs Scribe request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("ElevenLabs Scribe HTTP {status}: {body}");
    }

    let body: serde_json::Value = resp.json().await.context("parse Scribe response")?;
    let text = body["text"]
        .as_str()
        .context("missing 'text' field in Scribe response")?;

    Ok(text.to_string())
}
