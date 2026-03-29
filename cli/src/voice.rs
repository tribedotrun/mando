//! `mando voice` — voice control CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::Value;

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct VoiceArgs {
    #[command(subcommand)]
    pub command: VoiceCommand,
}

#[derive(Subcommand)]
pub(crate) enum VoiceCommand {
    /// Show TTS usage summary
    Usage {
        /// Include detailed per-request records
        #[arg(long)]
        detail: bool,
        /// Number of days to include (default 30)
        #[arg(long, default_value = "30")]
        days: u32,
    },
    /// List voice sessions
    Sessions,
}

pub(crate) async fn handle(args: VoiceArgs) -> anyhow::Result<()> {
    match args.command {
        VoiceCommand::Usage { detail, days } => handle_usage(detail, days).await,
        VoiceCommand::Sessions => handle_sessions().await,
    }
}

async fn handle_usage(detail: bool, days: u32) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut params = vec![format!("days={days}")];
    if detail {
        params.push("detail=true".into());
    }
    let path = format!("/api/voice/usage?{}", params.join("&"));
    let result = client.get(&path).await?;

    println!("Voice TTS Usage");
    println!("{}", "-".repeat(40));
    println!(
        "  Total requests:  {}",
        result["total_requests"].as_i64().unwrap_or(0)
    );
    println!(
        "  Total chars:     {}",
        result["total_chars"].as_i64().unwrap_or(0)
    );
    println!(
        "  Total errors:    {}",
        result["total_errors"].as_i64().unwrap_or(0)
    );
    println!(
        "  Avg latency:     {:.0}ms",
        result["avg_latency_ms"].as_f64().unwrap_or(0.0)
    );

    if detail {
        if let Some(records) = result["records"].as_array() {
            println!("\nRecent Records");
            println!(
                "{:<24}  {:<12}  {:>8}  {:>10}  ERROR",
                "TIMESTAMP", "VOICE", "CHARS", "LATENCY"
            );
            println!("{}", "-".repeat(70));
            for r in records {
                print_usage_record(r);
            }
            println!("\n{} record(s)", records.len());
        }
    }

    Ok(())
}

fn print_usage_record(r: &Value) {
    let ts = r["timestamp"].as_str().unwrap_or("?");
    let ts_short: String = ts.chars().take(19).collect();
    let voice = r["voice_id"].as_str().unwrap_or("?");
    let voice_short: String = voice.chars().take(12).collect();
    let chars = r["input_chars"].as_i64().unwrap_or(0);
    let latency = r["latency_ms"].as_i64().unwrap_or(0);
    let error = r["error"].as_str().unwrap_or("-");
    println!("{ts_short:<24}  {voice_short:<12}  {chars:>8}  {latency:>8}ms  {error}");
}

async fn handle_sessions() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/voice/sessions").await?;

    if let Some(pruned) = result["pruned"].as_u64() {
        if pruned > 0 {
            println!("Pruned {pruned} expired session(s).\n");
        }
    }

    let empty = vec![];
    let sessions = result["sessions"].as_array().unwrap_or(&empty);

    println!("{:<38}  {:<24}  TITLE", "SESSION_ID", "UPDATED");
    println!("{}", "-".repeat(80));

    for s in sessions {
        let id = s["id"].as_str().unwrap_or("?");
        let updated = s["updated_at"].as_str().unwrap_or("?");
        let updated_short: String = updated.chars().take(19).collect();
        let title = s["title"].as_str().unwrap_or("-");
        println!("{id:<38}  {updated_short:<24}  {title}");
    }

    println!("\n{} session(s)", sessions.len());
    Ok(())
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
        Voice(VoiceArgs),
    }

    #[test]
    fn parse_voice_usage() {
        let cli = TestCli::try_parse_from(["test", "voice", "usage"]).unwrap();
        match cli.cmd {
            TestCmd::Voice(args) => match args.command {
                VoiceCommand::Usage { detail, days } => {
                    assert!(!detail);
                    assert_eq!(days, 30);
                }
                _ => panic!("expected Usage"),
            },
        }
    }

    #[test]
    fn parse_voice_usage_detail() {
        let cli = TestCli::try_parse_from(["test", "voice", "usage", "--detail"]).unwrap();
        match cli.cmd {
            TestCmd::Voice(args) => match args.command {
                VoiceCommand::Usage { detail, .. } => {
                    assert!(detail);
                }
                _ => panic!("expected Usage"),
            },
        }
    }

    #[test]
    fn parse_voice_usage_days() {
        let cli = TestCli::try_parse_from(["test", "voice", "usage", "--days", "7"]).unwrap();
        match cli.cmd {
            TestCmd::Voice(args) => match args.command {
                VoiceCommand::Usage { days, .. } => {
                    assert_eq!(days, 7);
                }
                _ => panic!("expected Usage"),
            },
        }
    }

    #[test]
    fn parse_voice_sessions() {
        let cli = TestCli::try_parse_from(["test", "voice", "sessions"]).unwrap();
        match cli.cmd {
            TestCmd::Voice(args) => {
                assert!(matches!(args.command, VoiceCommand::Sessions));
            }
        }
    }
}
