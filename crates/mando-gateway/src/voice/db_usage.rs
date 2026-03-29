//! TTS usage tracking — delegates to mando_db::queries::voice.

use anyhow::Result;

use super::db::VoiceDb;

pub use mando_db::queries::voice::{TtsUsageRecord, UsageSummary};

pub(crate) struct TtsUsageEntry<'a> {
    pub session_id: Option<&'a str>,
    pub input_chars: i64,
    pub voice_id: &'a str,
    pub model: &'a str,
    pub latency_ms: i64,
    pub audio_duration_ms: Option<i64>,
    pub error: Option<&'a str>,
}

impl VoiceDb {
    pub async fn log_tts_usage(&self, entry: &TtsUsageEntry<'_>) -> Result<()> {
        mando_db::queries::voice::log_tts_usage(
            self.pool(),
            &mando_db::queries::voice::TtsUsageInput {
                session_id: entry.session_id,
                input_chars: entry.input_chars,
                voice_id: entry.voice_id,
                model: entry.model,
                latency_ms: entry.latency_ms,
                audio_duration_ms: entry.audio_duration_ms,
                error: entry.error,
            },
        )
        .await
    }

    pub async fn get_usage_summary(&self, days: u32) -> Result<UsageSummary> {
        mando_db::queries::voice::get_usage_summary(self.pool(), days).await
    }

    pub async fn get_usage_detail(&self, limit: usize, days: u32) -> Result<Vec<TtsUsageRecord>> {
        mando_db::queries::voice::get_usage_detail(self.pool(), limit, days).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> VoiceDb {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        VoiceDb::new(db.pool().clone())
    }

    #[tokio::test]
    async fn log_and_query_tts_usage() {
        let db = test_db().await;
        db.log_tts_usage(&TtsUsageEntry {
            session_id: None,
            input_chars: 100,
            voice_id: "voice1",
            model: "model1",
            latency_ms: 250,
            audio_duration_ms: Some(3000),
            error: None,
        })
        .await
        .unwrap();
        db.log_tts_usage(&TtsUsageEntry {
            session_id: None,
            input_chars: 50,
            voice_id: "voice1",
            model: "model1",
            latency_ms: 200,
            audio_duration_ms: None,
            error: Some("timeout"),
        })
        .await
        .unwrap();

        let summary = db.get_usage_summary(30).await.unwrap();
        assert_eq!(summary.total_requests, 2);
        assert_eq!(summary.total_chars, 150);
        assert_eq!(summary.total_errors, 1);
        assert!((summary.avg_latency_ms - 225.0).abs() < 0.01);

        let detail = db.get_usage_detail(10, 30).await.unwrap();
        assert_eq!(detail.len(), 2);
        assert_eq!(detail[0].input_chars, 50);
        assert_eq!(detail[0].error.as_deref(), Some("timeout"));
    }
}
