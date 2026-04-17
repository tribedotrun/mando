use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use tokio::time::timeout;

use crate::{Agent, CreateRequest, SessionState, TerminalEvent, TerminalHost, TerminalSize};

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mando-terminal-{label}-{}",
        global_infra::uuid::Uuid::v4()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn install_fake_claude(bin_dir: &PathBuf) {
    fs::create_dir_all(bin_dir).unwrap();
    let script = bin_dir.join("claude");
    fs::write(
        &script,
        "#!/bin/sh\nprintf 'READY:%s|%s\\n' \"$TEST_CONFIG\" \"$MANDO_TERMINAL_ID\"\nIFS= read -r line || exit 0\nprintf 'INPUT:%s\\n' \"$line\"\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(script, perms).unwrap();
}

async fn collect_until_exit(session: &crate::TerminalSession) -> (Vec<u8>, Option<u32>) {
    let mut rx = session.subscribe();
    let mut out = Vec::new();
    loop {
        match timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            TerminalEvent::Output(chunk) => out.extend_from_slice(&chunk),
            TerminalEvent::Exit { code } => return (out, code),
        }
    }
}

#[tokio::test]
async fn host_keeps_exited_session_history_distinct_from_restored_sessions() {
    let data_dir = temp_dir("restore");
    let bin_dir = data_dir.join("bin");
    install_fake_claude(&bin_dir);

    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let host = TerminalHost::new(data_dir.clone());
    let session = host
        .create(CreateRequest {
            project: "mando".into(),
            cwd: data_dir.clone(),
            agent: Agent::Claude,
            resume_session_id: None,
            size: Some(TerminalSize {
                rows: 30,
                cols: 100,
            }),
            config_env: HashMap::from([
                ("PATH".into(), path),
                ("TEST_CONFIG".into(), "config-value".into()),
            ]),
            terminal_env: HashMap::new(),
            terminal_id: Some("wb:panel".into()),
            extra_args: Vec::new(),
            name: None,
        })
        .unwrap();

    let session_id = session.info().id;
    session.write_input(b"hello world\n").await.unwrap();
    let (output, exit_code) = collect_until_exit(&session).await;
    let output = String::from_utf8(output).unwrap();
    assert!(
        output.contains(&format!("READY:config-value|{session_id}")),
        "expected MANDO_TERMINAL_ID to equal session id, got: {output}"
    );
    assert!(output.contains("INPUT:hello world"));
    assert_eq!(exit_code, Some(0));
    assert_eq!(session.info().state, SessionState::Exited);
    assert!(session.write_input(b"late input\n").await.is_err());

    drop(host);

    let restored_host = TerminalHost::new(data_dir.clone());
    let restored = restored_host.list();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].state, SessionState::Exited);
    assert!(!restored[0].restored);
    assert_eq!(restored[0].terminal_id.as_deref(), Some("wb:panel"));

    let _ = fs::remove_dir_all(data_dir);
}

#[test]
fn host_marks_unclean_history_as_restored() {
    let data_dir = temp_dir("unclean-restore");
    let history_root = data_dir.join("terminal-history/session-restore");
    fs::create_dir_all(&history_root).unwrap();
    fs::write(
        history_root.join("meta.json"),
        serde_json::json!({
            "id": "session-restore",
            "project": "mando",
            "cwd": data_dir,
            "agent": "claude",
            "terminal_id": "wb:restore",
            "created_at": "2026-04-08T00:00:00Z",
            "ended_at": null,
            "exit_code": null,
            "size": { "rows": 24, "cols": 80 },
            "state": "live"
        })
        .to_string(),
    )
    .unwrap();
    // scrollback.bin is no longer read but may exist from older sessions

    let host = TerminalHost::new(data_dir.clone());
    let sessions = host.list();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].state, SessionState::Restored);
    assert!(sessions[0].restored);
    assert_eq!(sessions[0].terminal_id.as_deref(), Some("wb:restore"));

    let _ = fs::remove_dir_all(data_dir);
}

#[test]
fn take_restorable_returns_only_restored_sessions() {
    let data_dir = temp_dir("take-restorable");
    let history_root = data_dir.join("terminal-history");

    // Session A: was live at crash (no ended_at) -> Restored -> should be taken
    let dir_a = history_root.join("session-a");
    fs::create_dir_all(&dir_a).unwrap();
    fs::write(
        dir_a.join("meta.json"),
        serde_json::json!({
            "id": "session-a",
            "project": "mando",
            "cwd": data_dir,
            "agent": "claude",
            "terminal_id": "wb:1",
            "created_at": "2026-04-10T00:00:00Z",
            "ended_at": null,
            "exit_code": null,
            "size": { "rows": 24, "cols": 80 },
            "state": "live",
            "name": "work-session"
        })
        .to_string(),
    )
    .unwrap();

    // Session B: exited normally -> Exited -> should NOT be taken
    let dir_b = history_root.join("session-b");
    fs::create_dir_all(&dir_b).unwrap();
    fs::write(
        dir_b.join("meta.json"),
        serde_json::json!({
            "id": "session-b",
            "project": "mando",
            "cwd": data_dir,
            "agent": "codex",
            "terminal_id": "wb:2",
            "created_at": "2026-04-10T00:00:00Z",
            "ended_at": "2026-04-10T00:05:00Z",
            "exit_code": 0,
            "size": { "rows": 24, "cols": 80 },
            "state": "exited"
        })
        .to_string(),
    )
    .unwrap();

    let host = TerminalHost::new(data_dir.clone());
    assert_eq!(host.list().len(), 2);

    let restorable = host.take_restorable();
    assert_eq!(restorable.len(), 1);
    assert_eq!(restorable[0].id, "session-a");
    assert_eq!(restorable[0].project, "mando");
    assert_eq!(restorable[0].terminal_id.as_deref(), Some("wb:1"));
    assert_eq!(restorable[0].name.as_deref(), Some("work-session"));

    // Only the exited session should remain
    let remaining = host.list();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "session-b");
    assert_eq!(remaining[0].state, SessionState::Exited);

    // History is NOT deleted by take_restorable (caller does it after
    // successful replacement), so both dirs still exist on disk.
    assert!(dir_a.exists());
    assert!(dir_b.exists());

    let _ = fs::remove_dir_all(data_dir);
}
