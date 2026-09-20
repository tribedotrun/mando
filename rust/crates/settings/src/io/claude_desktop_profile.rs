//! Local profile metadata and macOS Desktop process boundary.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileRegistration {
    pub user_data_dir: PathBuf,
    pub account_uuid: Option<String>,
}

pub(crate) fn profile_root() -> PathBuf {
    global_infra::paths::state_dir().join("claude-desktop")
}

pub(crate) fn shared_claude_dir() -> PathBuf {
    global_infra::paths::home_dir().join(".claude")
}

pub(crate) fn read_registration(id: i64) -> Result<Option<ProfileRegistration>> {
    let path = profile_root().join(format!("{id}.json"));
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes)
                .context("Invalid Claude Desktop profile registration")?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Read Claude Desktop profile registration"),
    }
}

pub(crate) fn write_registration(id: i64, registration: &ProfileRegistration) -> Result<()> {
    let root = profile_root();
    private_directory(&root)?;
    let mut file = tempfile::NamedTempFile::new_in(&root)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(registration)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join(format!("{id}.json")))?;
    Ok(())
}

pub(crate) fn private_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn account_uuid(path: &Path) -> Result<Option<String>> {
    let bytes = match std::fs::read(path.join("config.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Read Claude Desktop configuration"),
    };
    let config: serde_json::Value =
        serde_json::from_slice(&bytes).context("Invalid Claude Desktop configuration")?;
    match config.get("lastKnownAccountUuid") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if value.is_empty() => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => anyhow::bail!("Invalid Claude Desktop lastKnownAccountUuid"),
    }
}

/// Read exact main-process arguments, without inspecting environment or tokens.
#[cfg(target_os = "macos")]
pub(crate) async fn running_pid(path: &Path) -> Result<Option<u32>> {
    let output = tokio::process::Command::new("pgrep")
        .args(["-x", "Claude"])
        .output()
        .await?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    anyhow::ensure!(
        output.status.success(),
        "Failed to enumerate Claude processes"
    );
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        let pid: u32 = line.trim().parse()?;
        let Some((process_path, code_dir)) = process_profile_path(pid)? else {
            continue;
        };
        let process_path = process_path
            .canonicalize()
            .context("Resolve running Claude profile directory")?;
        let target = if path.exists() {
            path.canonicalize()
                .context("Resolve target Claude profile directory")?
        } else {
            path.to_path_buf()
        };
        if process_path == target {
            let expected = shared_claude_dir();
            let actual = code_dir.unwrap_or_else(|| expected.clone());
            let actual = if actual.exists() {
                actual.canonicalize()?
            } else {
                actual
            };
            let expected = if expected.exists() {
                expected.canonicalize()?
            } else {
                expected
            };
            anyhow::ensure!(actual == expected, "This Claude profile is running with a different CLAUDE_CONFIG_DIR. Quit it and reopen through Mando to share ~/.claude.");
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn running_pid(_path: &Path) -> Result<Option<u32>> {
    anyhow::bail!("Claude Desktop profiles require macOS")
}

#[cfg(target_os = "macos")]
pub(crate) async fn launch(path: &Path) -> Result<()> {
    if let Some(pid) = running_pid(path).await? {
        // Target the process, not the shared bundle ID (several accounts may be open).
        let script = format!("ObjC.import('AppKit'); if (!$.NSRunningApplication.runningApplicationWithProcessIdentifier({pid}).activateWithOptions(3)) throw new Error('Could not activate Claude');");
        let output = tokio::process::Command::new("osascript")
            .args(["-l", "JavaScript", "-e", &script])
            .output()
            .await?;
        anyhow::ensure!(output.status.success(), "Claude profile is already running, but macOS could not focus it. Select its window in the Dock.");
        return Ok(());
    }
    let app = Path::new("/Applications/Claude.app");
    anyhow::ensure!(
        app.is_dir(),
        "Install Claude Desktop in /Applications first"
    );
    let mut command = tokio::process::Command::new("open");
    for key in [
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CODE_EFFORT_LEVEL",
        "CLAUDE_USER_DATA_DIR",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
    ] {
        command.env_remove(key);
    }
    for key in [
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
    ] {
        command.arg("--env").arg(key);
    }
    let output = command
        .args(["-n", "-a"])
        .arg(app)
        .arg("--env")
        .arg(format!(
            "CLAUDE_CONFIG_DIR={}",
            shared_claude_dir().display()
        ))
        .args(["--env", "CLAUDE_CODE_EFFORT_LEVEL=xhigh"])
        .args(["--env", "CLAUDE_USER_DATA_DIR", "--args"])
        .arg(format!("--user-data-dir={}", path.display()))
        .output()
        .await?;
    anyhow::ensure!(output.status.success(), "Could not launch Claude Desktop");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn launch(_path: &Path) -> Result<()> {
    anyhow::bail!("Claude Desktop profiles require macOS")
}

/// KERN_PROCARGS2 preserves argument boundaries and lets us read only the profile
/// environment overrides. Never log its buffer: it can contain credentials.
#[cfg(target_os = "macos")]
fn process_profile_path(pid: u32) -> Result<Option<(PathBuf, Option<PathBuf>)>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, i32::try_from(pid)?];
    let mut size: libc::size_t = 0;
    // SAFETY: all pointers address live buffers of the supplied lengths.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error).context("Read Claude process argument size");
    }
    let mut bytes = vec![0_u8; size];
    // SAFETY: bytes owns size bytes and sysctl writes at most that many bytes.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error).context("Read Claude process arguments");
    }
    bytes.truncate(size);
    anyhow::ensure!(bytes.len() >= 4, "Truncated Claude process arguments");
    let count = i32::from_ne_bytes(bytes[..4].try_into()?);
    anyhow::ensure!(count > 0, "Invalid Claude argument count");
    let executable_end = bytes[4..]
        .iter()
        .position(|byte| *byte == 0)
        .context("Missing Claude executable path")?
        + 4;
    let executable = std::str::from_utf8(&bytes[4..executable_end])?;
    if !executable.ends_with("/Claude.app/Contents/MacOS/Claude") {
        return Ok(None);
    }
    let mut offset = executable_end;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut strings = bytes[offset..].split(|byte| *byte == 0);
    let mut arguments = Vec::new();
    for _ in 0..count {
        arguments.push(std::str::from_utf8(
            strings.next().context("Truncated Claude argument list")?,
        )?);
    }
    let mut selected = global_infra::paths::home_dir().join("Library/Application Support/Claude");
    for (index, argument) in arguments.iter().enumerate() {
        if let Some(value) = argument.strip_prefix("--user-data-dir=") {
            selected = PathBuf::from(value);
        } else if *argument == "--user-data-dir" {
            selected = PathBuf::from(
                arguments
                    .get(index + 1)
                    .context("Claude user-data-dir has no value")?,
            );
        }
    }
    let mut code_dir = None;
    for field in strings {
        if let Some(value) = field.strip_prefix(b"CLAUDE_CONFIG_DIR=") {
            if !value.is_empty() {
                code_dir = Some(PathBuf::from(std::str::from_utf8(value)?));
            }
        }
        if let Some(value) = field.strip_prefix(b"CLAUDE_USER_DATA_DIR=") {
            if !value.is_empty() {
                selected = PathBuf::from(std::str::from_utf8(value)?);
            }
        }
    }
    Ok(Some((selected, code_dir)))
}

/// Browser OAuth callbacks target the shared Claude bundle ID, so first login
/// needs other profiles closed. Authenticated profiles can run concurrently.
#[cfg(target_os = "macos")]
pub(crate) async fn other_profile_running(path: &Path) -> Result<bool> {
    let output = tokio::process::Command::new("pgrep")
        .args(["-x", "Claude"])
        .output()
        .await?;
    if output.status.code() == Some(1) {
        return Ok(false);
    }
    anyhow::ensure!(
        output.status.success(),
        "Failed to enumerate Claude processes"
    );
    let target = path
        .canonicalize()
        .context("Resolve target Claude profile")?;
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        if let Some((other, _)) = process_profile_path(line.trim().parse()?)? {
            if other
                .canonicalize()
                .context("Resolve running Claude profile")?
                != target
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn other_profile_running(_path: &Path) -> Result<bool> {
    anyhow::bail!("Claude Desktop profiles require macOS")
}
