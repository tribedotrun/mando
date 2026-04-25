//! Typed scout dashboard payloads that stay inside the scout crate boundary.

#[derive(Debug, Clone)]
pub enum ScoutActDraft {
    Skip {
        reason: String,
    },
    Create {
        task_title: String,
        task_description: String,
        project: String,
        scout_item_id: i64,
    },
}
