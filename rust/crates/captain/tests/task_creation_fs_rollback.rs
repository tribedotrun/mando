//! Filesystem-rollback integration test for `create_task_with_workbench`.
//!
//! `create_task_with_workbench` runs filesystem work (slot allocation +
//! `git worktree create` + bootstrap copy) before the DB transaction. If
//! anything inside the DB transaction fails, the function must clean up
//! the on-disk worktree and the branch so the next attempt sees a clean
//! slate.
//!
//! This test triggers a DB failure deterministically by handing the
//! creator a stale `project_id` — `workbenches.project_id` carries
//! `REFERENCES projects(id)` since migration 007, so the workbench
//! INSERT itself fails (the task INSERT is never reached). That's
//! exactly what we want: filesystem work has already finished and is
//! the only thing left to roll back. The unit test
//! `runtime::task_creation::tests::workbench_and_task_insert_rollback_together_on_task_failure`
//! covers the symmetric case where the task INSERT fails after the
//! workbench INSERT lands.
//!
//! Lives under `tests/` so its repository-setup `git init --bare` /
//! `git clone` / `git push` calls are exempt from
//! `check_git_cli_boundaries.py`.

use std::path::PathBuf;

fn run_git(args: &[&str], cwd: Option<&std::path::Path>) {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let status = cmd.status().expect("spawn git");
    assert!(status.success(), "git {args:?} (cwd={cwd:?}) failed");
}

fn temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", global_infra::uuid::Uuid::v4()))
}

#[tokio::test]
async fn create_task_with_workbench_cleans_up_filesystem_on_db_failure() {
    let bare_remote = temp_path("mando-fs-rollback-origin");
    let repo_root = temp_path("mando-fs-rollback");

    run_git(
        &[
            "init",
            "--bare",
            "-b",
            "main",
            bare_remote.to_str().unwrap(),
        ],
        None,
    );
    run_git(
        &[
            "clone",
            "--origin",
            "origin",
            bare_remote.to_str().unwrap(),
            repo_root.to_str().unwrap(),
        ],
        None,
    );
    run_git(
        &["config", "user.email", "test@example.com"],
        Some(&repo_root),
    );
    run_git(&["config", "user.name", "test"], Some(&repo_root));
    run_git(&["commit", "--allow-empty", "-m", "init"], Some(&repo_root));
    run_git(&["push", "-u", "origin", "main"], Some(&repo_root));
    run_git(&["remote", "set-head", "origin", "main"], Some(&repo_root));

    let pool_db = global_db::Db::open_in_memory().await.unwrap();
    let pool = pool_db.pool().clone();
    let real_project_id = settings::projects::upsert(
        &pool,
        "fs-rollback-test",
        repo_root.to_str().unwrap(),
        Some("acme/fs-rollback-test"),
    )
    .await
    .unwrap();

    let mut config = settings::Config::default();
    let pc = settings::ProjectConfig {
        name: "fs-rollback-test".into(),
        path: repo_root.to_string_lossy().into_owned(),
        github_repo: Some("acme/fs-rollback-test".into()),
        ..Default::default()
    };
    config
        .captain
        .projects
        .insert("fs-rollback-test".into(), pc);

    let mut task = captain::Task::new("fs rollback");
    task.project = "fs-rollback-test".into();
    // Stale project_id: `resolve_project_config` succeeds via the
    // project name (so FS work runs), but the workbench INSERT fails
    // on `workbenches.project_id REFERENCES projects(id)` (added by
    // migration 007). FS work is therefore the only thing that needs
    // to be rolled back.
    task.project_id = real_project_id + 999_999;

    let err = captain::create_task_with_workbench(&pool, &config, task)
        .await
        .expect_err("expected task INSERT to fail on stale project_id");
    assert!(
        err.to_string().contains("FOREIGN KEY")
            || err.chain().any(|e| e.to_string().contains("FOREIGN KEY")),
        "expected FK error, got: {err:#}"
    );

    let wb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workbenches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        wb_count, 0,
        "workbench INSERT must roll back when sibling task INSERT fails"
    );

    let stray_worktrees: Vec<_> = std::fs::read_dir(global_git::worktrees_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .contains(repo_root.file_name().unwrap().to_string_lossy().as_ref())
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        stray_worktrees.is_empty(),
        "filesystem rollback should leave no stray worktree under {}, found: {stray_worktrees:?}",
        repo_root.display()
    );

    let branches = std::process::Command::new("git")
        .args(["branch", "--list", "mando/*"])
        .current_dir(&repo_root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&branches.stdout);
    assert!(
        stdout.trim().is_empty(),
        "filesystem rollback should leave no mando/* branches, found: {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
    let _ = std::fs::remove_dir_all(&bare_remote);
}

/// Happy path for `create_task_with_workbench`: when everything succeeds,
/// both the workbench row and the task row exist with matching ids, the
/// on-disk worktree is created at the path stored on the workbench, and
/// the local `mando/<slug>` branch points at the worktree.
///
/// This locks in the contract that workbench INSERT and task INSERT
/// commit together — no orphan workbench (the DB-rollback unit test
/// covers the failure case; this covers the symmetric success case so
/// a regression that inserts only one of the two rows is caught at
/// Tier 1 instead of waiting for Tier 3 e2e.)
#[tokio::test]
async fn create_task_with_workbench_inserts_workbench_and_task_atomically() {
    let bare_remote = temp_path("mando-happy-path-origin");
    let repo_root = temp_path("mando-happy-path");

    run_git(
        &[
            "init",
            "--bare",
            "-b",
            "main",
            bare_remote.to_str().unwrap(),
        ],
        None,
    );
    run_git(
        &[
            "clone",
            "--origin",
            "origin",
            bare_remote.to_str().unwrap(),
            repo_root.to_str().unwrap(),
        ],
        None,
    );
    run_git(
        &["config", "user.email", "test@example.com"],
        Some(&repo_root),
    );
    run_git(&["config", "user.name", "test"], Some(&repo_root));
    run_git(&["commit", "--allow-empty", "-m", "init"], Some(&repo_root));
    run_git(&["push", "-u", "origin", "main"], Some(&repo_root));
    run_git(&["remote", "set-head", "origin", "main"], Some(&repo_root));

    let pool_db = global_db::Db::open_in_memory().await.unwrap();
    let pool = pool_db.pool().clone();
    let project_id = settings::projects::upsert(
        &pool,
        "happy-path",
        repo_root.to_str().unwrap(),
        Some("acme/happy-path"),
    )
    .await
    .unwrap();

    let mut config = settings::Config::default();
    let pc = settings::ProjectConfig {
        name: "happy-path".into(),
        path: repo_root.to_string_lossy().into_owned(),
        github_repo: Some("acme/happy-path".into()),
        ..Default::default()
    };
    config.captain.projects.insert("happy-path".into(), pc);

    let mut task = captain::Task::new("happy path");
    task.project = "happy-path".into();
    task.project_id = project_id;

    let task_id = captain::create_task_with_workbench(&pool, &config, task)
        .await
        .expect("happy-path create should succeed");
    assert!(task_id > 0);

    // Both rows exist; task points at the workbench just inserted.
    let row: (i64, i64, String) = sqlx::query_as(
        "SELECT t.id, t.workbench_id, w.worktree FROM tasks t \
         JOIN workbenches w ON w.id = t.workbench_id \
         WHERE t.id = ?",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, task_id);
    assert!(row.1 > 0, "workbench_id must be a real row, got {}", row.1);
    let stored_wt = std::path::PathBuf::from(&row.2);
    assert!(
        stored_wt.exists(),
        "stored worktree path must exist on disk: {}",
        stored_wt.display()
    );

    // Local `mando/<slug>` branch was created.
    let branches = std::process::Command::new("git")
        .args(["branch", "--list", "mando/*"])
        .current_dir(&repo_root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&branches.stdout);
    assert!(
        stdout.contains("mando/todo-"),
        "expected a mando/todo-<slot> branch on the repo, got: {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
    let _ = std::fs::remove_dir_all(&bare_remote);
}
