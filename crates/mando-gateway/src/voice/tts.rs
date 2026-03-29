//! ElevenLabs TTS integration — text-to-speech synthesis via ElevenLabs v1 API.
//!
//! API key read from `ELEVENLABS_API_KEY` env var (injected via config.json env section).

use anyhow::{bail, Context, Result};

const ELEVENLABS_BASE_URL: &str = "https://api.elevenlabs.io/v1/text-to-speech";

fn api_key() -> Result<String> {
    std::env::var("ELEVENLABS_API_KEY").context("ELEVENLABS_API_KEY not set")
}

/// Synthesize speech from text using ElevenLabs. Returns raw mp3 bytes.
pub(crate) async fn synthesize(text: &str, voice_id: &str, model: &str) -> Result<bytes::Bytes> {
    let key = api_key()?;

    let url = format!("{ELEVENLABS_BASE_URL}/{voice_id}?output_format=mp3_44100_128");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;

    let resp = client
        .post(&url)
        .header("xi-api-key", &key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "text": text,
            "model_id": model,
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.75,
                "speed": 1.2
            }
        }))
        .send()
        .await
        .with_context(|| format!("ElevenLabs API request failed for voice {voice_id}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("ElevenLabs API returned HTTP {status}: {body}");
    }

    resp.bytes()
        .await
        .context("failed to read ElevenLabs response bytes")
}
