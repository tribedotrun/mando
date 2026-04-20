//! Task artifact queries -- evidence snapshots, PR summaries, etc.

use anyhow::Result;
use sqlx::SqlitePool;

use crate::{ArtifactMedia, ArtifactType, TaskArtifact};

#[derive(sqlx::FromRow)]
struct ArtifactRow {
    id: i64,
    task_id: i64,
    artifact_type: String,
    content: String,
    media: Option<String>,
    created_at: String,
}

impl ArtifactRow {
    fn into_artifact(self) -> TaskArtifact {
        let artifact_type: ArtifactType =
            serde_json::from_value(serde_json::Value::String(self.artifact_type.clone()))
                .unwrap_or_else(|e| {
                    tracing::error!(
                        module = "captain-io-queries-artifacts", artifact_id = self.id,
                        raw_type = %self.artifact_type,
                        error = %e,
                        "unknown artifact type in DB -- defaulting to Evidence"
                    );
                    ArtifactType::Evidence
                });
        let media: Vec<ArtifactMedia> = self
            .media
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        TaskArtifact {
            id: self.id,
            task_id: self.task_id,
            artifact_type,
            content: self.content,
            media,
            created_at: self.created_at,
        }
    }
}

/// Insert a new artifact and return its auto-generated ID.
pub async fn insert(
    pool: &SqlitePool,
    task_id: i64,
    artifact_type: ArtifactType,
    content: &str,
    media: &[ArtifactMedia],
) -> Result<i64> {
    let type_str = serde_json::to_value(artifact_type)?
        .as_str()
        .unwrap_or("evidence")
        .to_string();
    let media_json = if media.is_empty() {
        None
    } else {
        Some(serde_json::to_string(media)?)
    };
    let ts = global_types::now_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO task_artifacts (task_id, artifact_type, content, media, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         RETURNING id",
    )
    .bind(task_id)
    .bind(&type_str)
    .bind(content)
    .bind(&media_json)
    .bind(&ts)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Load all artifacts for a task, ordered chronologically.
pub async fn list_for_task(pool: &SqlitePool, task_id: i64) -> Result<Vec<TaskArtifact>> {
    let rows: Vec<ArtifactRow> = sqlx::query_as(
        "SELECT id, task_id, artifact_type, content, media, created_at
         FROM task_artifacts WHERE task_id = ? ORDER BY created_at ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_artifact()).collect())
}

/// Load a single artifact by ID.
pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<TaskArtifact>> {
    let row: Option<ArtifactRow> = sqlx::query_as(
        "SELECT id, task_id, artifact_type, content, media, created_at
         FROM task_artifacts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.into_artifact()))
}

/// Update the media JSON for an artifact (e.g. backfill remote_url after GCS upload).
pub async fn update_media(pool: &SqlitePool, id: i64, media: &[ArtifactMedia]) -> Result<()> {
    let media_json = if media.is_empty() {
        None
    } else {
        Some(serde_json::to_string(media)?)
    };
    sqlx::query("UPDATE task_artifacts SET media = ?1 WHERE id = ?2")
        .bind(&media_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
