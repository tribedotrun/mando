//! macOS process control for the ChatGPT desktop app credential swap.
//!
//! Quits and relaunches `ChatGPT.app` around a slot-file swap, and warns
//! (without touching) about any Codex process running outside the app
//! bundle that could clobber the swap while it's in flight.
//!
//! `osascript` is deliberately never used to quit the app — it pops a
//! confirm dialog. `pkill -x ChatGPT` (SIGTERM by default) targets the
//! app's main process by exact name instead.

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;

/// Exact process name of the app's main process. Detection and termination
/// go through `pgrep -x` / `pkill -x` on this so they work regardless of
/// where the bundle is installed (`/Applications`, `~/Applications`, ...),
/// not just the default location.
/// macOS-only: the process-control paths that reference these are compiled
/// out on other platforms (where the public fns just bail).
#[cfg(target_os = "macos")]
const CHATGPT_PROCESS_NAME: &str = "ChatGPT";
/// Location-independent marker for the app's own helper processes (their
/// command lines contain the bundle sub-path), so the external-Codex scan
/// does not misflag them.
#[cfg(target_os = "macos")]
const CHATGPT_BUNDLE_MARKER: &str = "ChatGPT.app/Contents";
#[cfg(target_os = "macos")]
const QUIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
#[cfg(target_os = "macos")]
const QUIT_POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

/// Quit `ChatGPT.app` via SIGTERM, poll until its processes are gone, then
/// SIGKILL as a fallback. No-op when the app is not currently running.
#[cfg(target_os = "macos")]
pub(crate) async fn quit_chatgpt_app() -> Result<()> {
    if !is_chatgpt_running().await? {
        return Ok(());
    }

    // Targets the process named exactly "ChatGPT" (the app's main
    // process), not helper processes registered under other names.
    run_ignore_not_found("pkill", &["-x", CHATGPT_PROCESS_NAME]).await?;

    let deadline = tokio::time::Instant::now() + QUIT_POLL_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if !is_chatgpt_running().await? {
            return Ok(());
        }
        tokio::time::sleep(QUIT_POLL_INTERVAL).await;
    }

    if is_chatgpt_running().await? {
        run_ignore_not_found("pkill", &["-9", "-x", CHATGPT_PROCESS_NAME]).await?;
        tokio::time::sleep(QUIT_POLL_INTERVAL).await;
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn quit_chatgpt_app() -> Result<()> {
    anyhow::bail!("mando codex app-use/app-restore are macOS-only (ChatGPT desktop app)")
}

/// Relaunch `ChatGPT.app`. Best-effort: a failure here doesn't undo the
/// already-completed credential swap, so it is returned as a warning rather
/// than a hard error. Transport clients decide how to present that warning.
#[cfg(target_os = "macos")]
pub(crate) async fn relaunch_chatgpt_app() -> Option<String> {
    match tokio::process::Command::new("open")
        .args(["-a", CHATGPT_PROCESS_NAME])
        .status()
        .await
    {
        Ok(status) if status.success() => None,
        Ok(status) => Some(format!(
            "warning: `open -a ChatGPT` exited with {status}; relaunch it manually"
        )),
        Err(err) => Some(format!(
            "warning: failed to relaunch ChatGPT.app: {err}; relaunch it manually"
        )),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn relaunch_chatgpt_app() -> Option<String> {
    None
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

#[cfg(target_os = "macos")]
async fn is_chatgpt_running() -> Result<bool> {
    let status = tokio::process::Command::new("pgrep")
        .args(["-x", CHATGPT_PROCESS_NAME])
        .status()
        .await
        .context("failed to run `pgrep`")?;
    Ok(status.success())
}

/// Run a command, treating "no matching process" (pkill/pgrep exit code 1)
/// as success rather than an error.
#[cfg(target_os = "macos")]
async fn run_ignore_not_found(program: &str, args: &[&str]) -> Result<()> {
    let status = tokio::process::Command::new(program)
        .args(args)
        .status()
        .await
        .with_context(|| format!("failed to run `{program}`"))?;
    if status.success() || status.code() == Some(1) {
        return Ok(());
    }
    anyhow::bail!("`{program} {}` exited with {status}", args.join(" "));
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn external_scan_excludes_daemon_caller_and_chatgpt_bundle() {
        let lines = "10 /tmp/mando-gw codex\n20 mando codex app-use work\n30 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT codex\n40 codex app-server";
        assert_eq!(
            filter_external_codex_lines(lines, 10, Some(20)),
            vec!["40 codex app-server"]
        );
    }
}
