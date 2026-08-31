use std::path::PathBuf;

use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::Connection;

use super::*;

fn test_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mando-cli-{label}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ))
}

fn media(index: u32, filename: &str, local_path: &str) -> api_types::ArtifactMedia {
    api_types::ArtifactMedia {
        index,
        filename: filename.to_string(),
        ext: "png".to_string(),
        local_path: Some(local_path.to_string()),
        remote_url: None,
        caption: None,
        kind: None,
    }
}

#[test]
fn evidence_preflight_rejects_missing_source() {
    let dir = test_dir("evidence-preflight");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let present = dir.join("present.png");
    let missing = dir.join("missing.png");
    std::fs::write(&present, b"present").expect("write source");

    let error = preflight_evidence_sources(&[
        present.to_string_lossy().into_owned(),
        missing.to_string_lossy().into_owned(),
    ])
    .expect_err("missing source must fail preflight");

    global_infra::best_effort!(
        std::fs::remove_dir_all(&dir),
        "cleanup evidence preflight test dir"
    );
    assert!(error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    }));
}

#[tokio::test]
async fn copy_failure_removes_batch_files_and_rolls_back_metadata() {
    let dir = test_dir("evidence-rollback");
    let artifacts_dir = dir.join("artifacts/42");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");

    let options = SqliteConnectOptions::new()
        .filename(dir.join("mando.db"))
        .create_if_missing(true);
    let mut db = SqliteConnection::connect_with(&options)
        .await
        .expect("open test database");
    sqlx::query(
        "CREATE TABLE task_artifacts (
            id INTEGER PRIMARY KEY,
            task_id INTEGER NOT NULL,
            artifact_type TEXT NOT NULL
        )",
    )
    .execute(&mut db)
    .await
    .expect("create artifact table");
    sqlx::query("INSERT INTO task_artifacts VALUES (7, 42, 'evidence')")
        .execute(&mut db)
        .await
        .expect("seed artifact");

    let sources = [dir.join("one.png"), dir.join("two.png")];
    for source in &sources {
        std::fs::write(source, b"evidence").expect("write source");
    }
    let sources = sources.map(|path| path.to_string_lossy().into_owned());
    preflight_evidence_sources(&sources).expect("preflight sources");
    let staged = stage_evidence_sources(&artifacts_dir, &sources).expect("stage sources");
    let response = api_types::TaskEvidenceResponse {
        artifact_id: 7,
        task_id: 42,
        media: vec![
            media(0, "one.png", "artifacts/42/7-0.png"),
            media(1, "two.png", "artifacts/42/missing/7-1.png"),
        ],
    };

    let error = finalize_registered_evidence(&dir, 42, &response, &staged)
        .await
        .expect_err("second destination must fail");
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_artifacts WHERE id = 7")
        .fetch_one(&mut db)
        .await
        .expect("count artifacts");

    assert!(error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    }));
    assert_eq!(remaining, 0, "failed copy must remove its artifact row");
    assert!(!artifacts_dir.join("7-0.png").exists());

    drop(db);
    global_infra::best_effort!(
        std::fs::remove_dir_all(&dir),
        "cleanup evidence rollback test dir"
    );
}

#[tokio::test]
async fn metadata_registration_runs_after_staging_and_cleans_failed_batch() {
    let dir = test_dir("evidence-stage-before-register");
    let artifacts_dir = dir.join("artifacts/42");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");
    let source = dir.join("one.png");
    std::fs::write(&source, b"evidence").expect("write source");
    let sources = [source.to_string_lossy().into_owned()];
    let staged = stage_evidence_sources(&artifacts_dir, &sources).expect("stage sources");
    let staged_directory = staged.directory.clone();
    let staged_paths = staged.files.clone();

    let error = register_staged_evidence(staged, || async move {
        assert!(
            staged_paths.iter().all(|path| path.is_file()),
            "metadata registration must not start until every file is staged"
        );
        Err::<(), _>(anyhow::anyhow!("metadata unavailable"))
    })
    .await
    .expect_err("metadata failure must propagate");

    assert!(
        error.chain().count() >= 2,
        "registration error keeps context"
    );
    assert!(
        !staged_directory.exists(),
        "metadata failure must remove the staged batch"
    );
    global_infra::best_effort!(
        std::fs::remove_dir_all(&dir),
        "cleanup stage-before-register test dir"
    );
}
