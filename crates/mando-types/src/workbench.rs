use serde::{Deserialize, Serialize};

fn default_rev() -> i64 {
    1
}

/// Human-readable timestamp title for workbenches (e.g., "Apr 8 18:02").
pub fn workbench_title_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    let month = match now.month() {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    };
    format!(
        "{} {} {:02}:{:02}",
        month,
        now.day(),
        now.hour(),
        now.minute()
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workbench {
    pub id: i64,
    /// DB column: `project_id INTEGER NOT NULL REFERENCES projects(id)`.
    pub project_id: i64,
    /// Project name -- populated via JOIN on projects table.
    pub project: String,
    pub worktree: String,
    pub title: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(default = "default_rev")]
    pub rev: i64,
}

impl Workbench {
    pub fn new(project_id: i64, project: String, worktree: String, title: String) -> Self {
        Self {
            id: 0,
            project_id,
            project,
            worktree,
            title,
            created_at: crate::now_rfc3339(),
            pinned_at: None,
            archived_at: None,
            deleted_at: None,
            rev: 1,
        }
    }
}
