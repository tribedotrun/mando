use global_types::{SessionStatus, TaskProvider};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::{CallerGroup, SessionCaller};

/// A session row from the unified sessions table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub provider: TaskProvider,
    pub created_at: String,
    pub caller: String,
    pub cwd: String,
    pub model: String,
    pub status: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: i64,
    pub turn_count: i64,
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<String>,
    pub resumed_at: Option<String>,
    pub credential_id: Option<i64>,
    pub error: Option<String>,
    pub api_error_status: Option<i64>,
    /// RFC3339 timestamp set once a clarifier session's structured result
    /// has been applied to its parent task. `tick_clarify_poll` skips
    /// sessions where this is `Some` so a prior-round stream cannot be
    /// re-applied during the HTTP inline reclarifier window.
    pub result_applied_at: Option<String>,
}

impl<'r> sqlx::FromRow<'r, SqliteRow> for SessionRow {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        let provider_text: String = row.try_get("provider")?;
        let provider = provider_text
            .parse()
            .map_err(|e: String| sqlx::Error::ColumnDecode {
                index: "provider".into(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            })?;

        Ok(Self {
            session_id: row.try_get("session_id")?,
            provider,
            created_at: row.try_get("created_at")?,
            caller: row.try_get("caller")?,
            cwd: row.try_get("cwd")?,
            model: row.try_get("model")?,
            status: row.try_get("status")?,
            cost_usd: row.try_get("cost_usd")?,
            duration_ms: row.try_get("duration_ms")?,
            resumed: row.try_get("resumed")?,
            turn_count: row.try_get("turn_count")?,
            task_id: row.try_get("task_id")?,
            scout_item_id: row.try_get("scout_item_id")?,
            worker_name: row.try_get("worker_name")?,
            resumed_at: row.try_get("resumed_at")?,
            credential_id: row.try_get("credential_id")?,
            error: row.try_get("error")?,
            api_error_status: row.try_get("api_error_status")?,
            result_applied_at: row.try_get("result_applied_at")?,
        })
    }
}

impl SessionRow {
    /// Parse the caller string into the enum.
    pub fn parsed_caller(&self) -> Option<SessionCaller> {
        SessionCaller::parse(&self.caller)
    }

    /// Get the display group for this session.
    pub fn group(&self) -> Option<CallerGroup> {
        self.parsed_caller().map(|c| c.group())
    }
}

/// Input for upserting a session.
pub struct SessionUpsert<'a> {
    pub session_id: &'a str,
    pub provider: TaskProvider,
    pub created_at: &'a str,
    pub caller: &'a str,
    pub cwd: &'a str,
    pub model: &'a str,
    pub status: SessionStatus,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: bool,
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<&'a str>,
    pub resumed_at: Option<&'a str>,
    pub credential_id: Option<i64>,
    pub error: Option<&'a str>,
    pub api_error_status: Option<i64>,
}
