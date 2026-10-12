//! macOS process control scoped to the credential slot being changed.
//! Other Codex homes (for example a DeepSeek desktop profile) stay running.

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
const CHATGPT_PROCESS_NAME: &str = "ChatGPT";
#[cfg(target_os = "macos")]
const CHATGPT_BUNDLE_MARKER: &str = "ChatGPT.app/Contents";
#[cfg(target_os = "macos")]
const QUIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
#[cfg(target_os = "macos")]
const QUIT_POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

/// Only non-secret profile metadata is retained from the process environment.
#[cfg(target_os = "macos")]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DesktopProfile {
    app: PathBuf,
    home: PathBuf,
    user_data: Option<PathBuf>,
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct DesktopProfile;

/// Stop only processes using this credential home, retaining their launch profile.
/// An inspection or shutdown failure aborts before credentials can be changed.
#[cfg(target_os = "macos")]
pub(crate) async fn quit_chatgpt_app(home: &Path) -> Result<Vec<DesktopProfile>> {
    let home = home
        .canonicalize()
        .context("Resolve Codex credential home")?;
    // Include ancestors: Mando may itself have been launched from Codex.
    let output = tokio::process::Command::new("pgrep")
        .args(["-a", "-x", CHATGPT_PROCESS_NAME])
        .output()
        .await
        .context("Enumerate ChatGPT processes")?;
    anyhow::ensure!(
        output.status.success() || output.status.code() == Some(1),
        "Could not enumerate ChatGPT processes"
    );
    let mut selected = Vec::new();
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        let pid = line.trim().parse::<i32>()?;
        if let Some(profile) = process_profile(pid)? {
            if profile.home == home {
                selected.push((pid, profile));
            }
        }
    }
    for (pid, profile) in &selected {
        signal_profile(*pid, profile, libc::SIGTERM)?;
    }
    let deadline = tokio::time::Instant::now() + QUIT_POLL_TIMEOUT;
    loop {
        let mut running = false;
        for (pid, profile) in &selected {
            running |= process_profile(*pid)?.as_ref() == Some(profile);
        }
        if !running {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            for (pid, profile) in &selected {
                signal_profile(*pid, profile, libc::SIGKILL)?;
            }
            tokio::time::sleep(QUIT_POLL_INTERVAL).await;
            for (pid, profile) in &selected {
                anyhow::ensure!(
                    process_profile(*pid)?.as_ref() != Some(profile),
                    "Codex process {pid} did not stop; credentials were not changed"
                );
            }
            break;
        }
        tokio::time::sleep(QUIT_POLL_INTERVAL).await;
    }
    let mut profiles: Vec<_> = selected.into_iter().map(|(_, profile)| profile).collect();
    if profiles.is_empty() {
        profiles.push(DesktopProfile {
            app: PathBuf::from("ChatGPT"),
            home,
            user_data: None,
        });
    }
    Ok(profiles)
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn quit_chatgpt_app(_home: &Path) -> Result<Vec<DesktopProfile>> {
    anyhow::bail!("mando codex app-use/app-restore are macOS-only (ChatGPT desktop app)")
}

/// -n is necessary: another provider's desktop instance may still be running.
#[cfg(target_os = "macos")]
pub(crate) async fn relaunch_chatgpt_app(profiles: &[DesktopProfile]) -> Option<String> {
    for profile in profiles {
        let mut command = launch_command(profile);
        match command.status().await {
            Ok(status) if status.success() => {},
            Ok(status) => return Some(format!("warning: relaunching the selected ChatGPT profile exited with {status}; relaunch it manually")),
            Err(error) => return Some(format!("warning: failed to relaunch the selected ChatGPT profile: {error}")),
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn launch_command(profile: &DesktopProfile) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("open");
    command
        .env_remove("CODEX_HOME")
        .env_remove("CODEX_ELECTRON_USER_DATA_PATH");
    command
        .args(["-n", "-a"])
        .arg(&profile.app)
        .arg("--env")
        .arg(format!("CODEX_HOME={}", profile.home.display()))
        .arg("--env")
        .arg("CODEX_ELECTRON_USER_DATA_PATH");
    if let Some(user_data) = &profile.user_data {
        command
            .arg("--env")
            .arg(format!(
                "CODEX_ELECTRON_USER_DATA_PATH={}",
                user_data.display()
            ))
            .arg("--args")
            .arg(format!("--user-data-dir={}", user_data.display()));
    }
    command
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn relaunch_chatgpt_app(_profiles: &[DesktopProfile]) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn signal_profile(pid: i32, profile: &DesktopProfile, signal: i32) -> Result<()> {
    // Recheck identity before signaling, including after the graceful-quit wait.
    if process_profile(pid)?.as_ref() != Some(profile) {
        return Ok(());
    }
    // SAFETY: pid is a positive PID returned by pgrep, never a process group.
    if unsafe { libc::kill(pid, signal) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error).context("Signal selected Codex desktop process");
        }
    }
    Ok(())
}

/// Read NUL-delimited argv/environment without exposing credentials in ps output.
#[cfg(target_os = "macos")]
fn process_profile(pid: i32) -> Result<Option<DesktopProfile>> {
    anyhow::ensure!(pid > 0, "Invalid Codex process ID");
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    // SAFETY: all pointers address live buffers of the supplied lengths.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        if process_exited(pid, &error) {
            return Ok(None);
        }
        return Err(error).context("Read Codex process argument size");
    }
    let mut bytes = zeroize::Zeroizing::new(vec![0_u8; size]);
    // SAFETY: bytes owns size bytes and sysctl writes at most that many bytes.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        if process_exited(pid, &error) {
            return Ok(None);
        }
        return Err(error).context("Read Codex process arguments");
    }
    bytes.truncate(size);
    parse_profile(&bytes, &global_infra::paths::home_dir().join(".codex"))
}

/// KERN_PROCARGS2 returns EINVAL for zombies as well as genuine read errors.
/// Only treat it as exit when the kernel confirms zombie or absent state.
#[cfg(target_os = "macos")]
fn process_exited(pid: i32, error: &std::io::Error) -> bool {
    if error.raw_os_error() == Some(libc::ESRCH) {
        return true;
    }
    if error.raw_os_error() != Some(libc::EINVAL) {
        return false;
    }
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: info is writable for exactly size bytes; only read on a full write.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if written == size as i32 {
        // SAFETY: proc_pidinfo initialized the entire struct above.
        return unsafe { info.assume_init() }.pbi_status == libc::SZOMB;
    }
    // SAFETY: signal 0 only tests whether this positive PID exists.
    (unsafe { libc::kill(pid, 0) }) != 0
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(target_os = "macos")]
fn parse_profile(bytes: &[u8], default_home: &Path) -> Result<Option<DesktopProfile>> {
    anyhow::ensure!(bytes.len() >= 4, "Truncated Codex process arguments");
    let count = i32::from_ne_bytes(bytes[..4].try_into()?);
    anyhow::ensure!(count > 0, "Invalid Codex argument count");
    let executable_end = bytes[4..]
        .iter()
        .position(|byte| *byte == 0)
        .context("Missing Codex executable path")?
        + 4;
    let executable = Path::new(std::str::from_utf8(&bytes[4..executable_end])?);
    if !executable.ends_with("ChatGPT.app/Contents/MacOS/ChatGPT") {
        return Ok(None);
    }
    let app = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("Missing ChatGPT bundle path")?
        .to_path_buf();
    let mut offset = executable_end;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut strings = bytes[offset..].split(|byte| *byte == 0);
    let mut arguments = Vec::new();
    for _ in 0..count {
        arguments.push(std::str::from_utf8(
            strings.next().context("Truncated Codex argv")?,
        )?);
    }
    let mut user_data = None;
    for (index, argument) in arguments.iter().enumerate() {
        if let Some(value) = argument.strip_prefix("--user-data-dir=") {
            user_data = Some(PathBuf::from(value));
        } else if *argument == "--user-data-dir" {
            user_data = Some(PathBuf::from(
                arguments
                    .get(index + 1)
                    .context("Missing Codex user-data-dir")?,
            ));
        }
    }
    let mut home = default_home.to_path_buf();
    for field in strings {
        if let Some(value) = field
            .strip_prefix(b"CODEX_HOME=")
            .filter(|value| !value.is_empty())
        {
            home = PathBuf::from(std::str::from_utf8(value)?);
        }
        if let Some(value) = field
            .strip_prefix(b"CODEX_ELECTRON_USER_DATA_PATH=")
            .filter(|value| !value.is_empty())
        {
            user_data = Some(PathBuf::from(std::str::from_utf8(value)?));
        }
    }
    Ok(Some(DesktopProfile {
        app,
        home: home
            .canonicalize()
            .context("Resolve running Codex credential home")?,
        user_data,
    }))
}

/// Report (never kill) Codex-related processes running
/// outside the ChatGPT app bundle — e.g. a manually started `codex
/// app-server`. Those share `~/.codex/auth.json` and could clobber this
/// swap while it's in flight or shortly after.
#[cfg(target_os = "macos")]
pub(crate) async fn external_codex_process_warnings(caller_pid: Option<u32>) -> Vec<String> {
    match external_codex_processes(caller_pid).await {
        Ok(hits) if !hits.is_empty() => {
            let mut warnings = vec![format!(
                "WARNING: {} external Codex process(es) running outside the ChatGPT app bundle — they may clobber this swap (not stopping them):",
                hits.len()
            )];
            warnings.extend(hits.into_iter().map(|hit| format!("  {hit}")));
            warnings
        }
        Ok(_) => Vec::new(),
        Err(err) => vec![format!(
            "warning: failed to check for external Codex processes: {err}"
        )],
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn external_codex_process_warnings(_caller_pid: Option<u32>) -> Vec<String> {
    Vec::new()
}

/// List `pid command-line` entries for processes whose command line
/// contains "codex" but are neither running from inside the ChatGPT app
/// bundle, the daemon performing the scan, nor the thin CLI caller whose
/// argv contains the `codex` subcommand and would otherwise false-positive.
#[cfg(target_os = "macos")]
async fn external_codex_processes(caller_pid: Option<u32>) -> Result<Vec<String>> {
    let output = tokio::process::Command::new("pgrep")
        .args(["-fl", "codex"])
        .output()
        .await
        .context("failed to run `pgrep -fl codex`")?;
    // Exit code 1 means "no matches" — that's success for our purposes.
    if !output.status.success() && output.status.code() != Some(1) {
        anyhow::bail!("`pgrep -fl codex` exited with {}", output.status);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(filter_external_codex_lines(
        &text,
        std::process::id(),
        caller_pid,
    ))
}

#[cfg(target_os = "macos")]
fn filter_external_codex_lines(text: &str, own_pid: u32, caller_pid: Option<u32>) -> Vec<String> {
    let own_pid = own_pid.to_string();
    let caller_pid = caller_pid.map(|pid| pid.to_string());
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (pid, cmd) = line.split_once(' ')?;
            if pid == own_pid
                || caller_pid.as_deref() == Some(pid)
                || cmd.contains(CHATGPT_BUNDLE_MARKER)
            {
                None
            } else {
                Some(format!("{pid} {cmd}"))
            }
        })
        .collect()
}

// Exception to the no-unit-test default: exercising account swaps against the
// real app would interrupt live work and rotate real credentials. These tests
// use the production process reader/selector/signaler with disposable native
// processes and homes, never real auth files or the running desktop profile.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_selected_home_stops_and_profile_is_preserved() -> Result<()> {
        let root = tempfile::tempdir()?;
        let primary = root.path().join("normal home");
        let secondary = root.path().join("deepseek home");
        let alias = root.path().join("normal-alias");
        let executable = root.path().join("ChatGPT.app/Contents/MacOS/ChatGPT");
        std::fs::create_dir_all(executable.parent().context("fixture executable parent")?)?;
        std::fs::create_dir_all(&primary)?;
        std::fs::create_dir_all(&secondary)?;
        std::os::unix::fs::symlink(&primary, &alias)?;
        let fixture_source = root.path().join("idle.c");
        std::fs::write(
            &fixture_source,
            "#include <unistd.h>\nint main(void) { for (;;) pause(); }\n",
        )?;
        let compiled = std::process::Command::new("cc")
            .arg(&fixture_source)
            .arg("-o")
            .arg(&executable)
            .status()?;
        anyhow::ensure!(
            compiled.success(),
            "Could not build disposable process fixture"
        );
        let user_data = root.path().join("normal UI profile");
        let mut normal = tokio::process::Command::new(&executable)
            .arg("60")
            .env("CODEX_HOME", &alias)
            .env("CODEX_ELECTRON_USER_DATA_PATH", &user_data)
            .kill_on_drop(true)
            .spawn()?;
        let mut deepseek = tokio::process::Command::new(&executable)
            .arg("60")
            .env("CODEX_HOME", &secondary)
            .env(
                "CODEX_ELECTRON_USER_DATA_PATH",
                root.path().join("deepseek UI"),
            )
            .kill_on_drop(true)
            .spawn()?;
        let normal_pid = i32::try_from(normal.id().context("normal pid")?)?;
        let deepseek_pid = i32::try_from(deepseek.id().context("deepseek pid")?)?;
        // Let exec complete before inspecting the native executable metadata.
        for _ in 0..100 {
            if process_profile(normal_pid)?.is_some() && process_profile(deepseek_pid)?.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            process_profile(normal_pid)?.context("normal profile")?.home,
            primary.canonicalize()?
        );
        assert_eq!(
            process_profile(deepseek_pid)?
                .context("deepseek profile")?
                .home,
            secondary.canonicalize()?
        );
        // Reap our own child as launchd would reap the desktop process.
        let reaper = tokio::spawn(async move { normal.wait().await });
        let profiles = quit_chatgpt_app(&primary).await?;
        reaper.await??;
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].user_data.as_ref(), Some(&user_data));
        assert_eq!(profiles[0].app, root.path().join("ChatGPT.app"));
        let launch = launch_command(&profiles[0]);
        let args: Vec<_> = launch
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[..2], ["-n", "-a"]);
        assert!(args.contains(&format!("CODEX_HOME={}", primary.canonicalize()?.display())));
        assert!(args.contains(&format!("--user-data-dir={}", user_data.display())));
        assert!(args.contains(&format!(
            "CODEX_ELECTRON_USER_DATA_PATH={}",
            user_data.display()
        )));

        assert!(
            deepseek.try_wait()?.is_none(),
            "other provider must stay alive"
        );
        deepseek.kill().await?;
        deepseek.wait().await?;
        Ok(())
    }

    #[test]
    fn parses_default_home_and_argument_boundaries() -> Result<()> {
        let root = tempfile::tempdir()?;
        let exe = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";
        for arguments in [
            vec![exe, "--user-data-dir=/tmp/UI with spaces"],
            vec![exe, "--user-data-dir", "/tmp/UI with spaces"],
        ] {
            let mut bytes = (i32::try_from(arguments.len())?).to_ne_bytes().to_vec();
            bytes.extend(exe.as_bytes());
            bytes.extend([0, 0]);
            for argument in arguments {
                bytes.extend(argument.as_bytes());
                bytes.push(0);
            }
            bytes.extend(
                format!(
                    "CODEX_HOME={}\0SECRET=must-not-be-retained\0",
                    root.path().display()
                )
                .as_bytes(),
            );
            let profile = parse_profile(&bytes, root.path())?.context("profile")?;
            assert_eq!(profile.home, root.path().canonicalize()?);
            assert_eq!(
                profile.user_data,
                Some(PathBuf::from("/tmp/UI with spaces"))
            );
        }
        let mut bytes = 1_i32.to_ne_bytes().to_vec();
        bytes.extend(exe.as_bytes());
        bytes.extend([0, 0]);
        bytes.extend(exe.as_bytes());
        bytes.push(0);
        assert_eq!(
            parse_profile(&bytes, root.path())?
                .context("default profile")?
                .home,
            root.path().canonicalize()?
        );
        assert!(parse_profile(&[0, 1], root.path()).is_err());
        Ok(())
    }
}
