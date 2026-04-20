//! Session queries — re-exported from `sessions-db`.
pub use sessions_db::{
    category_counts, delete_sessions_for_task, find_session_id_by_worker_name, get_credential_id,
    is_session_running, list_running_sessions, list_running_sessions_for_task, list_sessions,
    list_sessions_for_scout_item, list_sessions_for_task, list_sessions_missing_cost,
    session_by_id, session_cwd, total_session_cost, update_session_status,
    update_session_status_with_cost, upsert_session, SessionRow, SessionUpsert,
};
