/// Convert a domain `Workbench` to the wire `api_types::WorkbenchItem` for
/// SSE bus broadcasts. Returns `Err` on schema drift so callers can
/// refuse to emit a corrupt event (fail-fast — the previous behavior of
/// emitting `item: None` papered over the drift and left the frontend
/// dispatching on a half-present payload).
pub fn to_wire_workbench_item(
    workbench: &crate::Workbench,
) -> anyhow::Result<api_types::WorkbenchItem> {
    let mut value = serde_json::to_value(workbench).map_err(|e| {
        anyhow::anyhow!(
            "failed to serialize Workbench {} for bus broadcast: {e}",
            workbench.id
        )
    })?;
    inject_worktree_exists(&mut value, &workbench.worktree, workbench.id)?;
    serde_json::from_value(value).map_err(|e| {
        anyhow::anyhow!(
            "failed to deserialize Workbench {} into api_types::WorkbenchItem (likely schema drift): {e}",
            workbench.id
        )
    })
}

/// Inject the derived `worktreeExists` field into a serialized workbench
/// JSON object. One source of truth for the filesystem probe used by every
/// `WorkbenchItem` wire path (HTTP routes, SSE snapshot, SSE bus broadcasts).
///
/// Semantics: returns `false` only when the daemon can affirmatively prove
/// the worktree is absent. Stat failures other than `NotFound` (permission,
/// stale NFS handle, unmounted volume) fail open to `true` with a warn log,
/// so a transient mount glitch never funnels the user into the
/// "Archive workbench" surface for a worktree they meant to keep.
fn inject_worktree_exists(
    value: &mut serde_json::Value,
    worktree: &str,
    workbench_id: i64,
) -> anyhow::Result<()> {
    let object = value.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("Workbench {workbench_id} did not serialize to a JSON object")
    })?;
    let exists = probe_worktree_exists(worktree, workbench_id);
    object.insert(
        "worktreeExists".to_string(),
        serde_json::Value::Bool(exists),
    );
    Ok(())
}

/// Filesystem probe for `inject_worktree_exists`. Sync `metadata` is
/// intentional: the call sites are bounded (one per workbench in a snapshot
/// or one per bus broadcast) and match the existing reconciler discipline.
fn probe_worktree_exists(worktree: &str, workbench_id: i64) -> bool {
    if worktree.is_empty() {
        return false;
    }
    match std::fs::metadata(worktree) {
        Ok(metadata) => metadata.is_dir(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            tracing::warn!(
                module = "captain-runtime-workbench_runtime",
                workbench_id,
                worktree,
                error = %err,
                "worktree stat failed for non-NotFound reason; reporting worktreeExists=true so users do not archive a worktree we could not check"
            );
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_marks_empty_path_as_missing() {
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, "", 7).unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(false));
    }

    #[test]
    fn inject_marks_real_dir_as_present() {
        let tmp = tempfile::tempdir().unwrap();
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, tmp.path().to_str().unwrap(), 7).unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(true));
    }

    #[test]
    fn inject_marks_missing_path_as_absent() {
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, "/this/path/should/never/exist/mando-test-42", 7)
            .unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(false));
    }

    #[test]
    fn inject_errors_on_non_object_value() {
        let mut value = serde_json::Value::String("nope".into());
        let result = inject_worktree_exists(&mut value, "/tmp", 7);
        assert!(result.is_err(), "non-object input must fail loudly");
    }
}
