//! `mando daemon` — daemon lifecycle management CLI.

use clap::{Args, Subcommand};

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommand {
    /// Start the daemon in the foreground
    Start {
        /// Port to listen on
        #[arg(short = 'p')]
        port: Option<u16>,
        /// Verbose logging
        #[arg(short = 'v')]
        verbose: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon health
    Health,
    /// Tail daemon log
    Logs {
        /// Number of lines to show
        #[arg(short = 'n', default_value = "50")]
        lines: usize,
        /// Follow mode (tail -f)
        #[arg(short = 'f')]
        follow: bool,
    },
}

pub(crate) async fn handle(args: DaemonArgs) -> anyhow::Result<()> {
    match args.command {
        DaemonCommand::Start { port, verbose } => handle_start(port, verbose).await,
        DaemonCommand::Stop => handle_stop().await,
        DaemonCommand::Health => handle_health().await,
        DaemonCommand::Logs { lines, follow } => handle_logs(lines, follow).await,
    }
}

async fn handle_start(port: Option<u16>, verbose: bool) -> anyhow::Result<()> {
    let port_str = port.map(|p| p.to_string());

    let mut cmd_args = vec!["--foreground"];
    if let Some(ref p) = port_str {
        cmd_args.push("--port");
        cmd_args.push(p);
    }

    let binary = find_daemon_binary();
    println!("Starting daemon ({})", binary.display());

    if verbose {
        std::env::set_var("RUST_LOG", "debug");
    }

    let err = exec_daemon(&binary, &cmd_args);
    anyhow::bail!("failed to exec daemon: {err}");
}

async fn handle_stop() -> anyhow::Result<()> {
    let data_dir = crate::http::data_dir();
    let pid_file = data_dir.join("daemon.pid");

    // Read PID from file (authoritative source).
    let pid = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    match pid {
        Some(pid) => {
            match tokio::process::Command::new("kill")
                .arg(pid.to_string())
                .status()
                .await
            {
                Ok(status) if status.success() => {
                    println!("Sent SIGTERM to daemon (pid {pid}).");
                }
                Ok(status) => {
                    eprintln!("Warning: kill exited with status {status} for pid {pid}.");
                }
                Err(e) => {
                    eprintln!("Warning: failed to send SIGTERM to pid {pid}: {e}");
                }
            }
        }
        None => {
            println!("No running daemon found (no PID file).");
        }
    }

    // Clean up both files. NotFound is expected; other errors are surfaced
    // so permission issues or a broken data dir don't pass silently.
    for path in [pid_file, data_dir.join("daemon.port")] {
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Warning: failed to remove {}: {e}", path.display());
            }
        }
    }

    Ok(())
}

async fn handle_health() -> anyhow::Result<()> {
    match DaemonClient::discover() {
        Ok(client) => match client.health().await {
            Ok(health) => {
                println!("Daemon is running.");
                println!("{}", serde_json::to_string_pretty(&health)?);
            }
            Err(e) => {
                println!("Daemon port file exists but not reachable: {e}");
            }
        },
        Err(_) => {
            println!("Daemon is not running.");
        }
    }
    Ok(())
}

async fn handle_logs(lines: usize, follow: bool) -> anyhow::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let log_path = std::path::PathBuf::from(home).join("Library/Logs/Mando/daemon.log");
    if !log_path.exists() {
        println!("No log file at {}", log_path.display());
        return Ok(());
    }

    let mut args = vec!["-n".to_string(), lines.to_string()];
    if follow {
        args.push("-f".to_string());
    }
    args.push(log_path.to_string_lossy().into_owned());

    let status = tokio::process::Command::new("tail")
        .args(&args)
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("tail exited with {status}");
    }
    Ok(())
}

/// Find the daemon binary (mando-gw or self binary).
fn find_daemon_binary() -> std::path::PathBuf {
    // Look for mando-gw next to current binary.
    if let Ok(self_path) = std::env::current_exe() {
        let dir = self_path.parent().unwrap_or(std::path::Path::new("."));
        let gw = dir.join("mando-gw");
        if gw.exists() {
            return gw;
        }
    }
    // Fallback: assume it's in PATH.
    std::path::PathBuf::from("mando-gw")
}

/// Exec the daemon binary, replacing this process.
#[cfg(unix)]
fn exec_daemon(binary: &std::path::Path, args: &[&str]) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    std::process::Command::new(binary).args(args).exec()
}

#[cfg(not(unix))]
fn exec_daemon(binary: &std::path::Path, args: &[&str]) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "daemon start only supported on Unix",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(clap::Subcommand)]
    enum TestCmd {
        Daemon(DaemonArgs),
    }

    #[test]
    fn parse_daemon_start() {
        let cli = TestCli::try_parse_from(["test", "daemon", "start"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => match args.command {
                DaemonCommand::Start { port, verbose } => {
                    assert!(port.is_none());
                    assert!(!verbose);
                }
                _ => panic!("expected Start"),
            },
        }
    }

    #[test]
    fn parse_daemon_start_port() {
        let cli = TestCli::try_parse_from(["test", "daemon", "start", "-p", "9999"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => match args.command {
                DaemonCommand::Start { port, .. } => {
                    assert_eq!(port, Some(9999));
                }
                _ => panic!("expected Start"),
            },
        }
    }

    #[test]
    fn parse_daemon_start_verbose() {
        let cli = TestCli::try_parse_from(["test", "daemon", "start", "-v"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => match args.command {
                DaemonCommand::Start { verbose, .. } => {
                    assert!(verbose);
                }
                _ => panic!("expected Start"),
            },
        }
    }

    #[test]
    fn parse_daemon_stop() {
        let cli = TestCli::try_parse_from(["test", "daemon", "stop"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => {
                assert!(matches!(args.command, DaemonCommand::Stop));
            }
        }
    }

    #[test]
    fn parse_daemon_health() {
        let cli = TestCli::try_parse_from(["test", "daemon", "health"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => {
                assert!(matches!(args.command, DaemonCommand::Health));
            }
        }
    }

    #[test]
    fn parse_daemon_logs() {
        let cli = TestCli::try_parse_from(["test", "daemon", "logs", "-n", "100"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => match args.command {
                DaemonCommand::Logs { lines, follow } => {
                    assert_eq!(lines, 100);
                    assert!(!follow);
                }
                _ => panic!("expected Logs"),
            },
        }
    }

    #[test]
    fn parse_daemon_logs_follow() {
        let cli = TestCli::try_parse_from(["test", "daemon", "logs", "-f"]).unwrap();
        match cli.cmd {
            TestCmd::Daemon(args) => match args.command {
                DaemonCommand::Logs { follow, .. } => {
                    assert!(follow);
                }
                _ => panic!("expected Logs"),
            },
        }
    }
}
