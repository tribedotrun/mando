//! Adopt recent local transcripts into a stopped Desktop profile without copying transcripts.
//!
//! Claude Desktop 2.2553.1 `recoverCliSessions` persists `local_<cliSessionId>`
//! records referencing the original CLI transcript. Its generic source importer instead
//! forks transcripts; that path does not provide shared history. We use only the minimal
//! local recovery fields, never cloud/bridge IDs, permissions, schedules or credentials.
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_TRANSCRIPT_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) struct LocalSessionSyncReport {
    pub imported: u32,
    pub eligible: u32,
    pub skipped: u32,
    pub journal_path: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdoptedIndex {
    session_id: String,
    cli_session_id: String,
    cwd: String,
    origin_cwd: String,
    title: String,
    created_at: u64,
    last_activity_at: u64,
    indexed_at: u64,
    is_archived: bool,
    adopted_from_other_surface: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportJournal {
    created_at: u64,
    recent_days: Option<u32>,
    entries: Vec<JournalEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalEntry {
    destination: String,
    // Exact planned contents permit safe manual rollback only while still unchanged.
    contents: String,
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn regular_entries(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.try_exists()? {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).with_context(|| format!("Read {}", path.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_reader(File::open(path)?)
        .with_context(|| format!("Invalid JSON in {}", path.display()))
}

fn live_sessions(shared: &Path) -> Result<HashSet<String>> {
    let mut live = HashSet::new();
    for path in regular_entries(&shared.join("sessions"))? {
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let value = read_json(&path)?;
        let pid = value
            .get("pid")
            .and_then(Value::as_i64)
            .filter(|p| *p > 0 && *p <= i32::MAX as i64)
            .context("Claude session marker has invalid pid; cannot establish writer liveness")?;
        // kill(pid, 0) checks existence only; it sends no signal. EPERM is live/unknown.
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
        if alive {
            let id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .context("Live Claude marker lacks sessionId; cannot safely import")?;
            live.insert(id.to_owned());
        }
    }
    Ok(live)
}

fn project_slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn trusted_projects(shared: &Path) -> Result<HashSet<String>> {
    // Claude's default global settings live beside ~/.claude, not inside it.
    let path = shared.with_extension("json");
    if !path.try_exists()? {
        return Ok(HashSet::new());
    }
    let value = read_json(&path)?;
    Ok(value
        .get("projects")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|projects| projects.iter())
        .filter(|(_, settings)| settings.get("hasTrustDialogAccepted") == Some(&Value::Bool(true)))
        .map(|(cwd, _)| cwd.clone())
        .collect())
}

fn transcript_index(
    path: &Path,
    now: u64,
    recent_days: Option<u32>,
    trusted: &HashSet<String>,
) -> Result<Option<AdoptedIndex>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_TRANSCRIPT_BYTES {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Ok(None);
        }
    }
    let id = path
        .file_stem()
        .and_then(|v| v.to_str())
        .context("Non-UTF8 session filename")?;
    if !is_uuid(id) {
        return Ok(None);
    }
    let mut cwd = None;
    let mut title = None;
    let mut created_at = None;
    let mut last_activity_at = None;
    let mut main_messages = false;
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(module = "claude-desktop-sessions", path = %path.display(), %error, "Skipping malformed Claude transcript");
                return Ok(None);
            }
        };
        if let Some(entry_cwd) = value.get("cwd").and_then(Value::as_str) {
            if !trusted.contains(entry_cwd)
                || !Path::new(entry_cwd).is_absolute()
                || !Path::new(entry_cwd).is_dir()
            {
                return Ok(None);
            }
            match &cwd {
                Some(first) if first != entry_cwd => return Ok(None),
                None => cwd = Some(entry_cwd.to_owned()),
                _ => {}
            }
        }
        if let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) {
            if let Ok(parsed) = time::OffsetDateTime::parse(
                timestamp,
                &time::format_description::well_known::Rfc3339,
            ) {
                let millis = parsed.unix_timestamp_nanos() / 1_000_000;
                if let Ok(millis) = u64::try_from(millis) {
                    created_at = Some(created_at.map_or(millis, |old: u64| old.min(millis)));
                    last_activity_at =
                        Some(last_activity_at.map_or(millis, |old: u64| old.max(millis)));
                }
            }
        }
        let kind = value.get("type").and_then(Value::as_str);
        let main = value.get("isSidechain") != Some(&Value::Bool(true));
        if main && matches!(kind, Some("user" | "assistant")) {
            main_messages = true;
        }
        if main
            && kind == Some("user")
            && title.is_none()
            && value.get("isMeta") != Some(&Value::Bool(true))
        {
            title = value
                .pointer("/message/content")
                .and_then(Value::as_str)
                .map(|text| text.chars().take(160).collect::<String>());
        }
        if matches!(kind, Some("custom-title" | "ai-title")) {
            if let Some(custom) = value
                .get("customTitle")
                .or_else(|| value.get("aiTitle"))
                .and_then(Value::as_str)
            {
                title = Some(custom.chars().take(160).collect());
            }
        }
    }
    let Some(cwd) = cwd else {
        return Ok(None);
    };
    let Some(last_activity_at) = last_activity_at else {
        return Ok(None);
    };
    if !main_messages
        || recent_days
            .is_some_and(|days| now.saturating_sub(last_activity_at) > u64::from(days) * 86_400_000)
    {
        return Ok(None);
    }
    // Desktop resolves shared transcripts by cwd slug. Do not create an index that
    // would point at a different project (or rely on its transient discovery cache).
    if path
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        != Some(project_slug(&cwd).as_str())
    {
        return Ok(None);
    }
    Ok(Some(AdoptedIndex {
        session_id: format!("local_{id}"),
        cli_session_id: id.to_owned(),
        origin_cwd: cwd.clone(),
        cwd,
        title: title.unwrap_or_else(|| "Local Code session".into()),
        created_at: created_at.unwrap_or(last_activity_at),
        last_activity_at,
        indexed_at: now,
        is_archived: false,
        adopted_from_other_surface: true,
    }))
}

fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_new(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().context("Index has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(contents)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

// Only presentation metadata is reused from the same account and organization.
// Cross-account adoption always starts from the shared transcript with local IDs.
fn same_account_metadata(
    sources: &[PathBuf],
    account: &str,
    org: &str,
) -> Result<std::collections::HashMap<String, Value>> {
    let mut records: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for source in sources {
        let directory = source.join("claude-code-sessions").join(account).join(org);
        for path in regular_entries(&directory)? {
            if !path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|name| name.starts_with("local_") && name.ends_with(".json"))
            {
                continue;
            }
            let value = read_json(&path)?;
            let Some(id) = value
                .get("cliSessionId")
                .and_then(Value::as_str)
                .filter(|id| is_uuid(id))
            else {
                continue;
            };
            if value.get("remoteTarget").is_some() || value.get("sshConfig").is_some() {
                continue;
            }
            let newer = records.get(id).is_none_or(|old| {
                value.get("lastActivityAt").and_then(Value::as_u64)
                    > old.get("lastActivityAt").and_then(Value::as_u64)
            });
            if newer {
                records.insert(id.to_owned(), value);
            }
        }
    }
    Ok(records)
}

/// Caller must hold its Desktop operation mutex and refuse a running destination.
/// Imports idle, trusted, recent LOCAL sessions from any account; no transcripts change.
pub(crate) fn sync_local_sessions(
    shared_claude_dir: &Path,
    destination_profile_dir: &Path,
    account_uuid: &str,
    organization_uuid: &str,
    recent_days: Option<u32>,
    preview: bool,
    source_profile_dirs: &[PathBuf],
) -> Result<LocalSessionSyncReport> {
    if !is_uuid(account_uuid) || !is_uuid(organization_uuid) {
        bail!("Invalid Desktop account or organization UUID");
    }
    let destination = destination_profile_dir
        .join("claude-code-sessions")
        .join(account_uuid)
        .join(organization_uuid);
    // Existing account/org tree establishes the target; do not guess an organization.
    if !destination.is_dir() {
        bail!("Open Claude Desktop once to establish this account's session directory");
    }
    let canonical_profile = destination_profile_dir.canonicalize()?;
    if !destination.canonicalize()?.starts_with(&canonical_profile) {
        bail!("Desktop session directory escapes profile");
    }
    let mut owned = HashSet::new();
    for path in regular_entries(&destination)? {
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if let Some(id) = name.strip_prefix("deleted_") {
            owned.insert(id.strip_prefix("local_").unwrap_or(id).to_owned());
            continue;
        }
        if !name.starts_with("local_") || !name.ends_with(".json") {
            continue;
        }
        let record = read_json(&path)?;
        for key in [
            "cliSessionId",
            "unarchivedCliSessionId",
            "preClearCliSessionId",
        ] {
            if let Some(id) = record.get(key).and_then(Value::as_str) {
                owned.insert(id.to_owned());
            }
        }
        if let Some(ids) = record.get("priorCliSessionIds").and_then(Value::as_array) {
            owned.extend(ids.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    let source_metadata =
        same_account_metadata(source_profile_dirs, account_uuid, organization_uuid)?;
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let trusted = trusted_projects(shared_claude_dir)?;
    let live = live_sessions(shared_claude_dir)?;
    let mut skipped = 0;
    let mut planned = Vec::new();
    let projects = shared_claude_dir.join("projects");
    if !projects.try_exists()? {
        return Ok(LocalSessionSyncReport {
            imported: 0,
            eligible: 0,
            skipped: 0,
            journal_path: None,
        });
    }
    for project in fs::read_dir(&projects)? {
        let project = project?;
        if !project.file_type()?.is_dir() {
            continue;
        }
        for path in regular_entries(&project.path())? {
            if path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|v| v.to_str())
                .context("Invalid transcript filename")?;
            if !is_uuid(id) {
                continue;
            }
            if owned.contains(id)
                || live.contains(id)
                || fs::metadata(&path)?
                    .modified()?
                    .elapsed()
                    .unwrap_or(Duration::ZERO)
                    < Duration::from_secs(60)
            {
                skipped += 1;
                continue;
            }
            let Some(mut index) = transcript_index(&path, now, recent_days, &trusted)? else {
                skipped += 1;
                continue;
            };
            if let Some(metadata) = source_metadata.get(&index.cli_session_id) {
                if let Some(title) = metadata.get("title").and_then(Value::as_str) {
                    index.title = title.to_owned();
                }
                if let Some(archived) = metadata.get("isArchived").and_then(Value::as_bool) {
                    index.is_archived = archived;
                }
            }
            let target = destination.join(format!("{}.json", index.session_id));
            if target.try_exists()? {
                skipped += 1;
                continue;
            }
            owned.insert(id.to_owned());
            planned.push((path, target, index));
        }
    }
    if planned.is_empty() {
        return Ok(LocalSessionSyncReport {
            imported: 0,
            eligible: 0,
            skipped,
            journal_path: None,
        });
    }
    let eligible = u32::try_from(planned.len())?;
    if preview {
        return Ok(LocalSessionSyncReport {
            imported: 0,
            eligible,
            skipped,
            journal_path: None,
        });
    }
    let journal_dir = destination_profile_dir.join("mando-session-imports");
    private_directory(&journal_dir)?;
    let journal_path = journal_dir.join(format!("{now}.json"));
    let journal = ImportJournal {
        created_at: now,
        recent_days,
        entries: planned
            .iter()
            .map(|(_, target, index)| {
                Ok(JournalEntry {
                    destination: target.to_string_lossy().into_owned(),
                    contents: serde_json::to_string_pretty(index)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    write_new(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
    let live = live_sessions(shared_claude_dir)?;
    let mut imported = 0;
    for ((source, target, index), entry) in planned.into_iter().zip(journal.entries) {
        if live.contains(&index.cli_session_id)
            || fs::metadata(&source)?
                .modified()?
                .elapsed()
                .unwrap_or(Duration::ZERO)
                < Duration::from_secs(60)
        {
            skipped += 1;
            continue;
        }
        write_new(&target, entry.contents.as_bytes())?;
        imported += 1;
    }
    tracing::info!(module = "claude-desktop-sessions", imported, skipped, journal = %journal_path.display(), "Imported shared local Claude session indexes");
    Ok(LocalSessionSyncReport {
        imported,
        eligible,
        skipped,
        journal_path: Some(journal_path.to_string_lossy().into_owned()),
    })
}

// Filesystem fixtures are intentional: third-party session indexes must never overwrite
// a transcript or inherit account-bound IDs. UI/API sandbox proof cannot cover these.
#[cfg(test)]
mod tests {
    use super::*;
    const ACCOUNT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const ORG: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const SESSION: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

    #[test]
    fn shared_import_preserves_transcript_and_deduplicates() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let shared = temp.path().join(".claude");
        let profile = temp.path().join("profile");
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd)?;
        let cwd_text = cwd.to_string_lossy().into_owned();
        let project = shared.join("projects").join(project_slug(&cwd_text));
        fs::create_dir_all(&project)?;
        let target = profile.join("claude-code-sessions").join(ACCOUNT).join(ORG);
        fs::create_dir_all(&target)?;
        fs::write(
            shared.with_extension("json"),
            serde_json::to_vec(&serde_json::json!({
                "projects": { cwd_text.clone(): { "hasTrustDialogAccepted": true } }
            }))?,
        )?;
        let timestamp = (time::OffsetDateTime::now_utc() - time::Duration::hours(2))
            .format(&time::format_description::well_known::Rfc3339)?;
        let transcript = format!(
            "{}\n",
            serde_json::json!({
                "type": "user", "cwd": cwd_text, "timestamp": timestamp,
                "sessionId": SESSION, "bridgeSessionIds": ["must-not-copy"],
                "message": { "content": "Shared history fixture" }
            })
        );
        let source = project.join(format!("{SESSION}.jsonl"));
        fs::write(&source, &transcript)?;
        File::options().write(true).open(&source)?.set_times(
            std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(120)),
        )?;
        let preview = sync_local_sessions(&shared, &profile, ACCOUNT, ORG, Some(30), true, &[])?;
        assert_eq!(preview.eligible, 1);
        assert_eq!(preview.imported, 0);
        assert!(!profile.join("mando-session-imports").exists());
        let tombstone = target.join(format!("deleted_{SESSION}"));
        fs::write(&tombstone, "1")?;
        assert_eq!(
            sync_local_sessions(&shared, &profile, ACCOUNT, ORG, Some(30), false, &[])?.imported,
            0
        );
        fs::remove_file(tombstone)?;
        let markers = shared.join("sessions");
        fs::create_dir_all(&markers)?;
        let marker = markers.join("fixture.json");
        fs::write(
            &marker,
            serde_json::to_vec(
                &serde_json::json!({ "pid": std::process::id(), "sessionId": SESSION }),
            )?,
        )?;
        assert_eq!(
            sync_local_sessions(&shared, &profile, ACCOUNT, ORG, Some(30), false, &[])?.imported,
            0
        );
        fs::remove_file(marker)?;
        let old_profile = temp.path().join("old-profile");
        let old_directory = old_profile
            .join("claude-code-sessions")
            .join(ACCOUNT)
            .join(ORG);
        fs::create_dir_all(&old_directory)?;
        fs::write(
            old_directory.join(format!("local_{SESSION}.json")),
            serde_json::to_vec(&serde_json::json!({
                "cliSessionId": SESSION, "title": "Preserved title", "isArchived": true,
                "bridgeSessionIds": ["never-copy"], "lastActivityAt": 1
            }))?,
        )?;
        let first = sync_local_sessions(
            &shared,
            &profile,
            ACCOUNT,
            ORG,
            Some(30),
            false,
            &[old_profile],
        )?;
        assert_eq!(first.imported, 1);
        assert!(first.journal_path.is_some());
        assert_eq!(fs::read_to_string(&source)?, transcript);
        let imported = target.join(format!("local_{SESSION}.json"));
        let mut record = read_json(&imported)?;
        assert_eq!(
            record.get("cliSessionId").and_then(Value::as_str),
            Some(SESSION)
        );
        assert_eq!(record["title"], "Preserved title");
        assert_eq!(record["isArchived"], true);
        assert!(record.get("bridgeSessionIds").is_none());
        assert!(record.get("oauthAccountAtSpawn").is_none());
        record["isArchived"] = Value::Bool(true);
        fs::write(&imported, serde_json::to_vec(&record)?)?;
        let second = sync_local_sessions(&shared, &profile, ACCOUNT, ORG, Some(30), false, &[])?;
        assert_eq!(second.imported, 0);
        assert_eq!(read_json(&imported)?["isArchived"], true);
        assert_eq!(fs::read_to_string(&source)?, transcript);
        Ok(())
    }

    #[test]
    fn transcript_filters_date_trust_and_live_writer() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd)?;
        let cwd_text = cwd.to_string_lossy().into_owned();
        let project = temp.path().join(project_slug(&cwd_text));
        fs::create_dir_all(&project)?;
        let source = project.join(format!("{SESSION}.jsonl"));
        let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let old = (time::OffsetDateTime::now_utc() - time::Duration::days(31))
            .format(&time::format_description::well_known::Rfc3339)?;
        fs::write(
            &source,
            format!(
                "{}\n",
                serde_json::json!({
                    "type": "user", "cwd": cwd_text, "timestamp": old, "message": {"content": "Old"}
                })
            ),
        )?;
        let trusted = HashSet::from([cwd_text.clone()]);
        assert!(transcript_index(&source, now, Some(30), &trusted)?.is_none());
        assert!(transcript_index(&source, now, None, &trusted)?.is_some());
        let recent = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        fs::write(
            &source,
            format!(
                "{}\n",
                serde_json::json!({
                    "type": "user", "cwd": cwd_text, "timestamp": recent, "message": {"content": "Recent"}
                })
            ),
        )?;
        assert!(transcript_index(&source, now, Some(30), &HashSet::new())?.is_none());
        assert!(transcript_index(&source, now, Some(30), &trusted)?.is_some());
        fs::create_dir(temp.path().join("sessions"))?;
        fs::write(
            temp.path().join("sessions/fixture.json"),
            serde_json::to_vec(&serde_json::json!({
                "pid": std::process::id(), "sessionId": SESSION
            }))?,
        )?;
        assert!(live_sessions(temp.path())?.contains(SESSION));
        Ok(())
    }
}
