//! Voice message and TTS usage queries.
//! Voice sessions are stored in the unified `sessions` table (caller = "voice-agent").
//! This module handles the voice-specific child tables.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

/// A voice message in a session.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct VoiceMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub action_name: Option<String>,
    pub action_result: Option<String>,
    pub created_at: String,
}

/// A TTS usage record.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TtsUsageRecord {
    pub id: i64,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub input_chars: i64,
    pub voice_id: String,
    pub model: String,
    pub latency_ms: i64,
    pub audio_duration_ms: Option<i64>,
    pub error: Option<String>,
}

/// TTS usage summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_chars: i64,
    pub total_errors: i64,
    pub avg_latency_ms: f64,
}

// ── Voice session helpers (creating voice sessions in the unified table) ─────

/// Create a new voice session in the unified sessions table.
pub async fn create_voice_session(pool: &SqlitePool) -> Result<String> {
    let id = mando_uuid::Uuid::v4().to_string();
    let now = mando_types::now_rfc3339();
    crate::queries::sessions::upsert_session(
        pool,
        &crate::queries::sessions::SessionUpsert {
            session_id: &id,
            created_at: &now,
            caller: "voice-agent",
            cwd: "",
            model: "",
            status: mando_types::SessionStatus::Running,
            cost_usd: None,
            duration_ms: None,
            resumed: false,
            task_id: None,
            scout_item_id: None,
            worker_name: None,
        },
    )
    .await?;
    Ok(id)
}

/// Get a voice session's title (derived from first user message).
pub async fn voice_session_title(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT content FROM voice_messages
         WHERE session_id = ? AND role = 'user'
         ORDER BY id ASC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(content,)| content.chars().take(50).collect()))
}

/// List voice sessions that have actual voice messages (excludes internal CC sessions).
pub async fn list_conversation_sessions(
    pool: &SqlitePool,
    limit: usize,
) -> Result<Vec<crate::queries::sessions::SessionRow>> {
    let rows: Vec<crate::queries::sessions::SessionRow> = sqlx::query_as(
        "SELECT s.session_id, s.created_at, s.caller, s.cwd, s.model, s.status,
                s.cost_usd, s.duration_ms, s.resumed, s.turn_count,
                s.task_id, s.scout_item_id, s.worker_name
         FROM cc_sessions s
         WHERE s.caller = 'voice-agent'
           AND EXISTS (SELECT 1 FROM voice_messages vm WHERE vm.session_id = s.session_id)
         ORDER BY s.created_at DESC LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Voice messages ──────────────────────────────────────────────────────────

/// Add a message to a voice session.
pub async fn add_message(
    pool: &SqlitePool,
    session_id: &str,
    role: &str,
    content: &str,
    action_name: Option<&str>,
    action_result: Option<&str>,
) -> Result<VoiceMessage> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "INSERT INTO voice_messages (session_id, role, content, action_name, action_result, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(role)
    .bind(content)
    .bind(action_name)
    .bind(action_result)
    .bind(&now)
    .execute(pool)
    .await?;
    let msg_id = result.last_insert_rowid();

    let msg: VoiceMessage = sqlx::query_as(
        "SELECT id, session_id, role, content, action_name, action_result, created_at
         FROM voice_messages WHERE id = ?",
    )
    .bind(msg_id)
    .fetch_one(pool)
    .await
    .context("message not found after insert")?;

    Ok(msg)
}

/// Get all messages for a session.
pub async fn get_messages(pool: &SqlitePool, session_id: &str) -> Result<Vec<VoiceMessage>> {
    let rows: Vec<VoiceMessage> = sqlx::query_as(
        "SELECT id, session_id, role, content, action_name, action_result, created_at
         FROM voice_messages WHERE session_id = ? ORDER BY id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete voice sessions whose last activity is older than `max_age_hours`.
///
/// Last activity is the latest voice message timestamp, falling back to
/// session created_at for sessions with no messages.
pub async fn prune_expired(pool: &SqlitePool, max_age_hours: u64) -> Result<u64> {
    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::hours(max_age_hours as i64);
    let cutoff_str = cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Find voice sessions whose last activity is before the cutoff.
    // Last activity = latest message created_at, or session created_at if no messages.
    let expired_query =
        "SELECT s.session_id FROM cc_sessions s
         WHERE s.caller = 'voice-agent'
           AND COALESCE(
               (SELECT MAX(vm.created_at) FROM voice_messages vm WHERE vm.session_id = s.session_id),
               s.created_at
           ) < ?";

    // Delete messages for expired voice sessions.
    sqlx::query(&format!(
        "DELETE FROM voice_messages WHERE session_id IN ({expired_query})"
    ))
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    // Delete the voice sessions themselves.
    let result = sqlx::query(&format!(
        "DELETE FROM cc_sessions WHERE session_id IN ({expired_query})"
    ))
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

// ── TTS usage ───────────────────────────────────────────────────────────────

/// Input for logging a TTS usage record.
pub struct TtsUsageInput<'a> {
    pub session_id: Option<&'a str>,
    pub input_chars: i64,
    pub voice_id: &'a str,
    pub model: &'a str,
    pub latency_ms: i64,
    pub audio_duration_ms: Option<i64>,
    pub error: Option<&'a str>,
}

/// Log a TTS usage record.
pub async fn log_tts_usage(pool: &SqlitePool, input: &TtsUsageInput<'_>) -> Result<()> {
    let now = mando_types::now_rfc3339();
    sqlx::query(
        "INSERT INTO voice_tts_usage
         (session_id, timestamp, input_chars, voice_id, model, latency_ms, audio_duration_ms, error)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.session_id)
    .bind(&now)
    .bind(input.input_chars)
    .bind(input.voice_id)
    .bind(input.model)
    .bind(input.latency_ms)
    .bind(input.audio_duration_ms)
    .bind(input.error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Aggregate TTS usage summary.
pub async fn get_usage_summary(pool: &SqlitePool, days: u32) -> Result<UsageSummary> {
    let cutoff = super::cutoff_rfc3339(i64::from(days));
    let row: (i64, i64, i64, f64) = sqlx::query_as(
        "SELECT
            COUNT(*) as total_requests,
            COALESCE(SUM(input_chars), 0) as total_chars,
            COUNT(CASE WHEN error IS NOT NULL THEN 1 END) as total_errors,
            COALESCE(AVG(latency_ms), 0.0) as avg_latency_ms
         FROM voice_tts_usage WHERE timestamp >= ?",
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;
    Ok(UsageSummary {
        total_requests: row.0,
        total_chars: row.1,
        total_errors: row.2,
        avg_latency_ms: row.3,
    })
}

/// Detailed TTS usage records.
pub async fn get_usage_detail(
    pool: &SqlitePool,
    limit: usize,
    days: u32,
) -> Result<Vec<TtsUsageRecord>> {
    let cutoff = super::cutoff_rfc3339(i64::from(days));
    let rows: Vec<TtsUsageRecord> = sqlx::query_as(
        "SELECT id, session_id, timestamp, input_chars, voice_id, model,
                latency_ms, audio_duration_ms, error
         FROM voice_tts_usage WHERE timestamp >= ?
         ORDER BY id DESC LIMIT ?",
    )
    .bind(&cutoff)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
