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

/// Typed role for an evidence media file. Mirrors `api_types::EvidenceKind`.
/// `None` (legacy) is treated by captain as `Other` for gate purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    #[serde(rename = "before_fix")]
    BeforeFix,
    #[serde(rename = "after_fix")]
    AfterFix,
    #[serde(rename = "cannot_reproduce")]
    CannotReproduce,
    #[serde(rename = "other")]
    Other,
}

impl From<api_types::EvidenceKind> for EvidenceKind {
    fn from(k: api_types::EvidenceKind) -> Self {
        match k {
            api_types::EvidenceKind::BeforeFix => Self::BeforeFix,
            api_types::EvidenceKind::AfterFix => Self::AfterFix,
            api_types::EvidenceKind::CannotReproduce => Self::CannotReproduce,
            api_types::EvidenceKind::Other => Self::Other,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Typed role: `BeforeFix` / `AfterFix` for bug-fix tasks, `Other` (or
    /// `None` legacy) otherwise. Captain gates the bug-fix evidence rule
    /// off this field rather than caption text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<EvidenceKind>,
}

#[derive(Debug, Clone)]
pub struct EvidenceFileSpec {
    pub filename: String,
    pub ext: String,
    pub caption: String,
    pub kind: Option<EvidenceKind>,
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

#[derive(Debug, Clone)]
pub struct EvidenceArtifactCreated {
    pub artifact_id: i64,
    pub media: Vec<ArtifactMedia>,
}

#[derive(Debug, Clone)]
pub enum UpdateArtifactMediaOutcome {
    Updated,
    ArtifactNotFound,
    MediaIndexNotFound(u32),
}
