//! Shared session DB layer.
//!
//! Contains the session query functions, types, and lifecycle helpers used by
//! both `sessions` and `captain`. Extracted so that `captain` does not need to
//! take a direct Cargo dependency on `sessions`.

mod caller;
mod lifecycle;
mod queries;

pub use caller::{CallerGroup, SessionCaller};
pub use lifecycle::{infer_command, SessionLifecycleCommand};
pub use queries::{
    category_counts, delete_sessions_for_task, find_session_id_by_worker_name, get_credential_id,
    is_session_running, list_running_sessions, list_running_sessions_for_task, list_sessions,
    list_sessions_for_scout_item, list_sessions_for_task, list_sessions_missing_cost,
    session_by_id, session_cwd, total_session_cost, update_session_status,
    update_session_status_with_cost, upsert_session, SessionRow, SessionUpsert,
};
