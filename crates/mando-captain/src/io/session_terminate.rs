//! Unified session termination — kills process, updates all liveness sources.

use mando_types::SessionStatus;
use sqlx::SqlitePool;

/// Terminate a session: kill process, update DB, stream meta, PID registry, health store.
/// No-op if session is already stopped/failed.
/// Attempts process kill even if DB is unavailable — a dead process should be
/// killed regardless of whether we can read its session row.
pub async fn terminate_session(
    pool: &SqlitePool,
    session_id: &str,
    new_status: SessionStatus,
    health_state: Option<&mut super::health_store::HealthState>,
) {
    // 1. Check if session is running. If DB is unavailable, proceed with kill
    //    anyway — fail-open on the kill side, not the cleanup side.
    match mando_db::queries::sessions::is_session_running(pool, session_id).await {
        Ok(false) => return,
        Err(e) => {
            tracing::warn!(
                module = "session_terminate",
                session_id,
                error = %e,
                "DB query failed — proceeding with kill attempt"
            );
        }
        Ok(true) => {}
    }

    // 2. Kill process via pid_registry (fingerprint-verified to avoid PID reuse).
    if let Some(pid) = super::pid_registry::get_verified_pid(session_id) {
        if pid.as_u32() > 0 && mando_cc::is_process_alive(pid) {
            if let Err(e) = mando_cc::kill_process(pid).await {
                tracing::warn!(
                    module = "session_terminate",
                    session_id,
                    pid = %pid,
                    error = %e,
                    "kill failed"
                );
            }
        }
    }

    // 3. Read cost/duration from stream file before updating DB.
    let stream_path = mando_config::stream_path_for_session(session_id);
    let cost_info = mando_cc::get_stream_cost(&stream_path);
    let (cost_usd, duration_ms, num_turns, denials, model_usage) = match &cost_info {
        Some(info) => (
            info.cost_usd,
            info.duration_ms.map(|d| d as i64),
            info.num_turns,
            info.permission_denials_count,
            info.model_usage.clone(),
        ),
        None => (None, None, None, None, None),
    };

    // 4. Update cc_sessions status + cost. DB failure must not block local
    //    cleanup -- a stale DB row is recoverable via reconciliation, but a
    //    leaked PID or health entry is not.
    let db_ok = match mando_db::queries::sessions::update_session_status_with_cost(
        pool,
        session_id,
        new_status,
        cost_usd,
        duration_ms,
        num_turns,
    )
    .await
    {
        Ok(()) => true,
        Err(e) => {
            tracing::error!(
                module = "session_terminate",
                session_id,
                error = %e,
                "failed to update session status — session may appear stale until next reconciliation"
            );
            false
        }
    };

    // 5. Update stream meta.
    mando_cc::update_stream_meta_status(session_id, "stopped", cost_usd);

    // 6. Unregister PID. Always, even if DB update failed.
    if let Err(e) = super::pid_registry::unregister(session_id) {
        tracing::warn!(
            module = "session_terminate",
            session_id,
            error = %e,
            "failed to unregister pid"
        );
    }

    // 7. Remove health entry by worker_name if health_state provided.
    if let Some(hs) = health_state {
        if let Ok(Some(row)) = mando_db::queries::sessions::session_by_id(pool, session_id).await {
            if let Some(ref wn) = row.worker_name {
                hs.remove(wn);
            }
        }
    }

    // Serialize model_usage to a compact JSON string for structured logging.
    // Absent when the CLI didn't emit per-model breakdown.
    let model_usage_log = model_usage
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();
    if db_ok {
        tracing::info!(
            module = "session_terminate",
            session_id,
            status = %new_status.as_str(),
            cost_usd = ?cost_usd,
            duration_ms = ?duration_ms,
            num_turns = ?num_turns,
            permission_denials = ?denials,
            model_usage = %model_usage_log,
            "session terminated"
        );
    } else {
        tracing::warn!(
            module = "session_terminate",
            session_id,
            status = %new_status.as_str(),
            permission_denials = ?denials,
            "session terminated (PID + health cleaned, DB update failed)"
        );
    }
}
