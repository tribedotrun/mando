//! Task artifact types -- evidence snapshots, work summaries.

use serde::{Deserialize, Serialize};

/// Type of a task artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactType {
    #[serde(rename = "evidence")]
    Evidence,
    #[serde(rename = "work_summary")]
    WorkSummary,
}

/// A single media attachment in an artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMedia {
    /// Positional index within the artifact (0-based).
    pub index: u32,
    /// Original or generated filename (e.g. "screenshot.png").
    pub filename: String,
    /// File extension without dot (e.g. "png", "mp4", "gif").
    pub ext: String,
    /// Path relative to data_dir (e.g. "artifacts/42/7-0.png").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Remote URL (GCS or GitHub PR attachment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Per-file caption describing what this media shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// A task artifact stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskArtifact {
    pub id: i64,
    pub task_id: i64,
    pub artifact_type: ArtifactType,
    pub content: String,
    pub media: Vec<ArtifactMedia>,
    pub created_at: String,
}
