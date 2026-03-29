//! `mando cron` — cron job management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct CronArgs {
    #[command(subcommand)]
    pub command: CronCommand,
}

#[derive(Subcommand)]
pub(crate) enum CronCommand {
    /// List cron jobs
    List {
        /// Include disabled jobs
        #[arg(short = 'a')]
        all: bool,
    },
    /// Add a new cron job
    Add {
        /// Job name
        #[arg(short = 'n')]
        name: String,
        /// Message / payload
        #[arg(short = 'm')]
        message: String,
        /// Interval (e.g. "30m", "1h", "5s")
        #[arg(long)]
        every: Option<String>,
        /// Cron expression (e.g. "*/5 * * * *")
        #[arg(long)]
        cron: Option<String>,
        /// Timezone (e.g. "America/Mexico_City")
        #[arg(long)]
        tz: Option<String>,
    },
    /// Enable a cron job
    Enable {
        /// Job ID
        id: String,
    },
    /// Disable a cron job
    Disable {
        /// Job ID
        id: String,
    },
    /// Delete a cron job
    Delete {
        /// Job ID
        id: String,
    },
    /// Manually run a cron job (test)
    Test {
        /// Job ID
        id: String,
    },
}

pub(crate) async fn handle(args: CronArgs) -> anyhow::Result<()> {
    match args.command {
        CronCommand::List { all } => handle_list(all).await,
        CronCommand::Add {
            name,
            message,
            every,
            cron,
            tz,
        } => handle_add(&name, &message, every, cron, tz).await,
        CronCommand::Enable { id } => handle_toggle(&id, true).await,
        CronCommand::Disable { id } => handle_toggle(&id, false).await,
        CronCommand::Delete { id } => handle_delete(&id).await,
        CronCommand::Test { id } => handle_test(&id).await,
    }
}

async fn handle_list(include_disabled: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let path = if include_disabled {
        "/api/cron?all=true"
    } else {
        "/api/cron"
    };
    let result = client.get(path).await?;
    let jobs = result["jobs"].as_array();

    println!(
        "{:<20}  {:<10}  {:<8}  {:<10}  NEXT RUN",
        "NAME", "SCHEDULE", "ENABLED", "STATUS"
    );
    println!("{}", "-".repeat(70));

    if let Some(jobs) = jobs {
        for job in jobs {
            let name = job["name"].as_str().unwrap_or("?");
            let kind = job["schedule_kind"].as_str().unwrap_or("?");
            let enabled = job["enabled"].as_bool().unwrap_or(false);
            let status = job["last_status"].as_str().unwrap_or("-");
            let next = job["next_run_at_ms"]
                .as_i64()
                .map(format_epoch_ms)
                .unwrap_or_else(|| "-".into());
            println!(
                "{name:<20}  {kind:<10}  {:<8}  {status:<10}  {next}",
                if enabled { "yes" } else { "no" },
            );
        }
    }

    Ok(())
}

async fn handle_add(
    name: &str,
    message: &str,
    every: Option<String>,
    cron_expr: Option<String>,
    tz: Option<String>,
) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let (schedule_kind, schedule_value) = if let Some(ref v) = every {
        ("every", v.as_str())
    } else if let Some(ref v) = cron_expr {
        ("cron", v.as_str())
    } else {
        anyhow::bail!("provide --every or --cron");
    };
    let mut body = json!({
        "name": name,
        "message": message,
        "schedule_kind": schedule_kind,
        "schedule_value": schedule_value,
    });
    if let Some(ref v) = tz {
        body["tz"] = json!(v);
    }

    let result = client.post("/api/cron/add", &body).await?;
    println!("Created cron job:");
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn handle_toggle(id: &str, enabled: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": id, "enabled": enabled});
    client.post("/api/cron/toggle", &body).await?;
    let state = if enabled { "enabled" } else { "disabled" };
    println!("Job {id} is now {state}.");
    Ok(())
}

async fn handle_delete(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": id});
    client.post("/api/cron/remove", &body).await?;
    println!("Deleted job {id}.");
    Ok(())
}

async fn handle_test(id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let body = json!({"id": id});
    client.post("/api/cron/run", &body).await?;
    println!("Test run complete for job {id}.");
    Ok(())
}

/// Format epoch milliseconds as a human-readable string.
fn format_epoch_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let delta = secs - now;
    if delta < 0 {
        format!("{}s ago", -delta)
    } else if delta < 60 {
        format!("in {delta}s")
    } else if delta < 3600 {
        format!("in {}m", delta / 60)
    } else {
        format!("in {}h", delta / 3600)
    }
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
        Cron(CronArgs),
    }

    #[test]
    fn parse_cron_list() {
        let cli = TestCli::try_parse_from(["test", "cron", "list"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::List { all } => assert!(!all),
                _ => panic!("expected List"),
            },
        }
    }

    #[test]
    fn parse_cron_list_all() {
        let cli = TestCli::try_parse_from(["test", "cron", "list", "-a"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::List { all } => assert!(all),
                _ => panic!("expected List"),
            },
        }
    }

    #[test]
    fn parse_cron_add_every() {
        let cli = TestCli::try_parse_from([
            "test", "cron", "add", "-n", "test-job", "-m", "hello", "--every", "30m",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Add {
                    name,
                    message,
                    every,
                    cron,
                    ..
                } => {
                    assert_eq!(name, "test-job");
                    assert_eq!(message, "hello");
                    assert_eq!(every.as_deref(), Some("30m"));
                    assert!(cron.is_none());
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_cron_add_cron_expr() {
        let cli = TestCli::try_parse_from([
            "test",
            "cron",
            "add",
            "-n",
            "tick",
            "-m",
            "run",
            "--cron",
            "*/5 * * * *",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Add { cron, .. } => {
                    assert_eq!(cron.as_deref(), Some("*/5 * * * *"));
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_cron_enable() {
        let cli = TestCli::try_parse_from(["test", "cron", "enable", "job-1"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Enable { id } => assert_eq!(id, "job-1"),
                _ => panic!("expected Enable"),
            },
        }
    }

    #[test]
    fn parse_cron_disable() {
        let cli = TestCli::try_parse_from(["test", "cron", "disable", "job-1"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Disable { id } => assert_eq!(id, "job-1"),
                _ => panic!("expected Disable"),
            },
        }
    }

    #[test]
    fn parse_cron_delete() {
        let cli = TestCli::try_parse_from(["test", "cron", "delete", "job-1"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Delete { id } => assert_eq!(id, "job-1"),
                _ => panic!("expected Delete"),
            },
        }
    }

    #[test]
    fn parse_cron_test() {
        let cli = TestCli::try_parse_from(["test", "cron", "test", "job-1"]).unwrap();
        match cli.cmd {
            TestCmd::Cron(args) => match args.command {
                CronCommand::Test { id } => assert_eq!(id, "job-1"),
                _ => panic!("expected Test"),
            },
        }
    }

    #[test]
    fn format_epoch_future() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let result = format_epoch_ms(now + 90_000);
        assert!(result.starts_with("in "));
    }

    #[test]
    fn format_epoch_past() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let result = format_epoch_ms(now - 5_000);
        assert!(result.ends_with("ago"));
    }
}
