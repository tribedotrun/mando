//! VoiceDb — thin async wrapper around mando_db voice queries.
//!
//! Voice sessions live in the unified `sessions` table (caller = "voice-agent").
//! Voice messages and TTS usage live in their own tables, managed by mando_db.

use anyhow::Result;
use sqlx::SqlitePool;

pub use mando_db::queries::voice::VoiceMessage;

pub(crate) struct VoiceDb {
    pool: SqlitePool,
}

impl VoiceDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn create_session(&self) -> Result<String> {
        mando_db::queries::voice::create_voice_session(&self.pool).await
    }

    pub async fn get_session_exists(&self, id: &str) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT session_id FROM cc_sessions WHERE session_id = ? AND caller = 'voice-agent'",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        action_name: Option<&str>,
        action_result: Option<&str>,
    ) -> Result<VoiceMessage> {
        mando_db::queries::voice::add_message(
            &self.pool,
            session_id,
            role,
            content,
            action_name,
            action_result,
        )
        .await
    }

    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<VoiceMessage>> {
        mando_db::queries::voice::get_messages(&self.pool, session_id).await
    }

    pub async fn prune_expired(&self, max_age_hours: u64) -> Result<u64> {
        mando_db::queries::voice::prune_expired(&self.pool, max_age_hours).await
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
    async fn create_and_check_session() {
        let db = test_db().await;
        let id = db.create_session().await.unwrap();
        assert!(!id.is_empty());
        assert!(db.get_session_exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn add_message_and_get() {
        let db = test_db().await;
        let id = db.create_session().await.unwrap();

        let msg = db
            .add_message(&id, "user", "Hello, how are you?", None, None)
            .await
            .unwrap();
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello, how are you?");

        let messages = db.get_messages(&id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello, how are you?");
    }

    #[tokio::test]
    async fn get_messages_ordered() {
        let db = test_db().await;
        let id = db.create_session().await.unwrap();

        db.add_message(&id, "user", "one", None, None)
            .await
            .unwrap();
        db.add_message(&id, "assistant", "two", Some("captain_status"), None)
            .await
            .unwrap();
        db.add_message(&id, "user", "three", None, None)
            .await
            .unwrap();

        let messages = db.get_messages(&id).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "one");
        assert_eq!(messages[1].content, "two");
        assert_eq!(messages[1].action_name.as_deref(), Some("captain_status"));
        assert_eq!(messages[2].content, "three");
    }

    #[tokio::test]
    async fn get_nonexistent_session() {
        let db = test_db().await;
        assert!(!db.get_session_exists("nonexistent").await.unwrap());
    }
}
