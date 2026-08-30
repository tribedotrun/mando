//! The detached Claude session both async captain phases run.
//!
//! Captain review and captain merge each hand a rendered prompt to a
//! headless CC process that outlives the tick. Everything around that call —
//! eager session logging, pid registration, session retargeting, result and
//! failure logging, rate-limit checks and the panic guard — is identical; only
//! the tools, schema, timeout and session slot differ.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use global_types::SessionStatus;
use tracing::{info, warn};

use super::notify::Notifier;
use super::session_retarget::retarget_session_id;

/// Everything one detached captain CC session needs.
pub(super) struct DetachedClaudeSession {
    /// `cc_sessions.caller` for this phase, e.g. `captain-review-async`.
    pub(super) caller: &'static str,
    /// Human-readable phase name used in log lines, e.g. `captain review`.
    pub(super) phase: &'static str,
    pub(super) session_id: String,
    pub(super) task_id: i64,
    pub(super) cwd: PathBuf,
    pub(super) prompt: String,
    pub(super) model: String,
    pub(super) timeout: std::time::Duration,
    pub(super) cc_max_retries: u32,
    pub(super) effort: global_claude::Effort,
    pub(super) allowed_tools: Vec<String>,
    pub(super) disallowed_tools: Vec<String>,
    pub(super) json_schema: serde_json::Value,
    pub(super) slot: crate::SessionSlot,
    pub(super) credential: Option<(i64, String)>,
    pub(super) notifier: Notifier,
    pub(super) pool: sqlx::SqlitePool,
}

/// Every session id one detached run has spawned under, oldest first.
///
/// Attempt 0 keeps the caller's pre-allocated id; each retry drops it and CC
/// mints a fresh one. Both the poller re-point and the teardown (pid
/// unregister, failure result, session-row settle) need to know which id is
/// live and which ids this run has left behind, so the spawn hook records
/// them here as they appear.
#[derive(Clone)]
pub(super) struct SpawnedSessionIds {
    ids: Arc<Mutex<Vec<String>>>,
}

impl SpawnedSessionIds {
    pub(super) fn new(preallocated: String) -> Self {
        Self {
            ids: Arc::new(Mutex::new(vec![preallocated])),
        }
    }

    /// A lock this process poisoned is still readable: the guarded value is a
    /// plain `Vec<String>` with no invariant a panic could have broken, and
    /// losing the id list would strand a pid registration and a running
    /// session row.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.ids.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record the id an attempt actually spawned under. Returns the id it
    /// supersedes when CC adopted a new one, which is what the poller's slot
    /// still holds and therefore what the retarget compares against.
    pub(super) fn record(&self, sid: &str) -> Option<String> {
        let mut ids = self.lock();
        let previous = ids.last()?;
        if previous == sid {
            return None;
        }
        let previous = previous.clone();
        ids.push(sid.to_string());
        Some(previous)
    }

    /// The id the live attempt is writing its stream under — the one the
    /// poller's slot points at once the retarget lands.
    pub(super) fn effective(&self) -> String {
        self.lock().last().cloned().unwrap_or_default()
    }

    /// Ids earlier attempts spawned under. Each left a `running` session row
    /// and a pid registration behind that nothing else will settle.
    pub(super) fn superseded(&self) -> Vec<String> {
        let ids = self.lock();
        ids.split_last()
            .map(|(_, rest)| rest.to_vec())
            .unwrap_or_default()
    }
}

/// Log the session as running, then detach it.
///
/// TRACKED: the spawned CC session is not registered with the gateway's
/// TaskTracker because captain is a library crate with no dependency on
/// AppState. On shutdown the external CC process is killed via the pid
/// registry; the task writes its result to the stream file, which persists
/// across restarts, so no in-memory state is lost.
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = session.task_id, caller = session.caller))]
pub(super) async fn spawn_detached_claude_session(session: DetachedClaudeSession) {
    // Log the "running" session entry before detaching so (a) cancel can find
    // it immediately and (b) the timeline never references a missing session.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        &session.pool,
        &session.session_id,
        &session.cwd,
        &session.model,
        session.caller,
        "",
        Some(session.task_id),
        false,
        session.credential.as_ref().map(|c| c.0),
    )
    .await
    {
        let sid = &session.session_id;
        warn!(module = "captain", %sid, %e, "failed to log running session");
    }

    let spawned = SpawnedSessionIds::new(session.session_id.clone());
    let spawned_for_panic = spawned.clone();
    let phase = session.phase;
    tokio::spawn(async move {
        let result = AssertUnwindSafe(run_session(session, spawned))
            .catch_unwind()
            .await;

        if let Err(panic) = result {
            // Resolve the id the last attempt was running under, not the
            // pre-allocated one: after a retry the poller watches the id CC
            // minted, and an error result on any other stream is invisible.
            let session_id = spawned_for_panic.effective();
            tracing::error!(
                module = "captain",
                %session_id,
                "{phase} spawn panicked: {:?}",
                panic
            );
            let stream_path = global_infra::paths::stream_path_for_session(&session_id);
            global_claude::write_error_result(
                &stream_path,
                &format!("{phase} spawn panicked: {panic:?}"),
            );
        }
    });
}

async fn run_session(session: DetachedClaudeSession, spawned: SpawnedSessionIds) {
    let DetachedClaudeSession {
        caller,
        phase,
        session_id,
        task_id,
        cwd,
        prompt,
        model,
        timeout,
        cc_max_retries,
        effort,
        allowed_tools,
        disallowed_tools,
        json_schema,
        slot,
        credential,
        notifier,
        pool,
    } = session;

    let builder = global_claude::CcConfig::builder()
        .model(&model)
        .effort(effort)
        .timeout(timeout)
        .caller(caller)
        .task_id(task_id.to_string())
        .cwd(cwd.clone())
        .session_id(session_id.clone())
        .allowed_tools(allowed_tools)
        .disallowed_tools(disallowed_tools)
        .json_schema(json_schema);
    let config = global_claude::with_credential(builder, &credential).build();

    let pool_for_hook = pool.clone();
    let spawned_for_hook = spawned.clone();
    let outcome = global_claude::CcOneShot::run_with_retry_pid_hook(
        &prompt,
        config,
        cc_max_retries,
        |pid, effective_sid| {
            if let Err(e) = crate::io::pid_registry::register(effective_sid, pid) {
                warn!(module = "captain", sid = %effective_sid, %e, "pid_registry register failed");
            }
            // Compare against the id the previous attempt claimed, not the
            // pre-allocated one: a second retry would otherwise find the slot
            // already re-pointed and be rejected by the compare-and-swap.
            if let Some(previous) = spawned_for_hook.record(effective_sid) {
                retarget_session_id(&pool_for_hook, task_id, slot, &previous, effective_sid);
            }
        },
    )
    .await;

    // The live attempt's id. After a retry this is the id CC minted, which is
    // both what the poller's slot now holds and where CC wrote its stream.
    let effective_sid = spawned.effective();
    for sid in spawned.superseded() {
        unregister_pid(&sid);
        settle_superseded_session(&pool, &sid).await;
    }
    unregister_pid(&effective_sid);

    match outcome {
        Ok(result) => {
            info!(
                module = "captain",
                session_id = %effective_sid,
                preallocated_session_id = %session_id,
                cost_usd = result.cost_usd.unwrap_or(0.0),
                duration_ms = result.duration_ms.unwrap_or(0),
                stream_file_bytes = stream_size(&result.stream_path),
                "{phase} CC completed"
            );
            // Read the credential from the pre-allocated row: it is the one
            // `log_running_session` stamped, and every retry ran on the same
            // credential because they all share this run's config.
            let cred_id = sessions_db::get_credential_id(&pool, &session_id)
                .await
                .unwrap_or(None);
            notifier.check_rate_limit(&result, &pool, cred_id).await;
            if let Err(e) =
                crate::io::headless_cc::log_cc_result(&pool, &result, &cwd, caller, Some(task_id))
                    .await
            {
                warn!(module = "captain", session_id = %effective_sid, %e, "log_cc_result failed");
            }
        }
        Err(e) => {
            let stream_path = global_infra::paths::stream_path_for_session(&effective_sid);
            warn!(
                module = "captain",
                session_id = %effective_sid,
                preallocated_session_id = %session_id,
                stream_file_bytes = stream_size(&stream_path),
                error = %e,
                "{phase} CC failed"
            );
            // Write the synthetic result to the stream the poller is actually
            // watching so the phase resolves on the next tick instead of
            // riding out its full timeout.
            global_claude::write_error_result(
                &stream_path,
                &format!("{phase} CC process failed: {e}"),
            );
            let error_text = format!("{e}");
            let api_error_status = e.api_error_status();
            if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                &pool,
                &effective_sid,
                &cwd,
                caller,
                Some(task_id),
                Some(&error_text),
                api_error_status,
            )
            .await
            {
                warn!(module = "captain", session_id = %effective_sid, %e2, "log_cc_failure failed");
            }
        }
    }
}

/// Close out the eager `running` row an attempt that CC then abandoned left
/// behind. Without this the row stays `running` forever and keeps counting
/// against its credential's live-session load.
async fn settle_superseded_session(pool: &sqlx::SqlitePool, session_id: &str) {
    if let Err(e) = crate::io::headless_cc::log_session_completion(
        pool,
        session_id,
        "",
        "",
        "",
        None,
        SessionStatus::Failed,
    )
    .await
    {
        warn!(module = "captain", %session_id, %e, "failed to settle superseded session row");
    }
}

fn stream_size(stream_path: &std::path::Path) -> u64 {
    std::fs::metadata(stream_path)
        .map(|m| m.len())
        .unwrap_or(u64::MAX)
}

fn unregister_pid(session_id: &str) {
    if let Err(e) = crate::io::pid_registry::unregister(session_id) {
        warn!(module = "captain", %session_id, %e, "pid_registry unregister failed");
    }
}

#[cfg(test)]
mod tests {
    use super::SpawnedSessionIds;

    #[test]
    fn attempt_zero_keeps_the_preallocated_id_and_needs_no_retarget() {
        let spawned = SpawnedSessionIds::new("preallocated".into());
        assert_eq!(spawned.record("preallocated"), None);
        assert_eq!(spawned.effective(), "preallocated");
        assert!(spawned.superseded().is_empty());
    }

    #[test]
    fn a_retry_supersedes_the_preallocated_id() {
        let spawned = SpawnedSessionIds::new("preallocated".into());
        spawned.record("preallocated");
        assert_eq!(spawned.record("cc-minted"), Some("preallocated".into()));
        assert_eq!(spawned.effective(), "cc-minted");
        assert_eq!(spawned.superseded(), vec!["preallocated".to_string()]);
    }

    /// The retarget is a compare-and-swap against what the slot currently
    /// holds, so a second retry must compare against the first retry's id —
    /// not the pre-allocated one, which the slot no longer carries.
    #[test]
    fn a_second_retry_supersedes_the_first_retrys_id() {
        let spawned = SpawnedSessionIds::new("preallocated".into());
        spawned.record("preallocated");
        spawned.record("cc-minted-1");
        assert_eq!(spawned.record("cc-minted-2"), Some("cc-minted-1".into()));
        assert_eq!(spawned.effective(), "cc-minted-2");
        assert_eq!(
            spawned.superseded(),
            vec!["preallocated".to_string(), "cc-minted-1".to_string()]
        );
    }
}
