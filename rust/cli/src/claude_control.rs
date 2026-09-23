//! Private, authenticated local launcher control channel. Never transports credentials.
use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub(crate) const SOCKET_ENV: &str = "MANDO_CLAUDE_CONTROL_SOCKET";
pub(crate) const SECRET_ENV: &str = "MANDO_CLAUDE_CONTROL_SECRET";
pub(crate) const GENERATION_ENV: &str = "MANDO_CLAUDE_GENERATION";

#[derive(Args)]
pub(crate) struct SwitchArgs {
    /// Saved Mando profile label. Omit to route automatically.
    pub profile: Option<String>,
    /// Explain the destination without reserving or switching.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlRequest {
    pub secret: String,
    pub generation: String,
    pub command: ControlCommand,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ControlCommand {
    Switch {
        profile: Option<String>,
        dry_run: bool,
    },
    Started {
        session_start: bool,
        session_id: String,
        cwd: String,
        model: Option<String>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlResponse {
    pub ok: bool,
    pub message: String,
}

pub(crate) async fn handle_switch(args: SwitchArgs) -> Result<()> {
    if args
        .profile
        .as_ref()
        .is_some_and(|profile| profile.trim().is_empty())
    {
        bail!("profile label must not be empty");
    }
    let response = send(ControlCommand::Switch {
        profile: args.profile,
        dry_run: args.dry_run,
    })
    .await?;
    if !response.ok {
        bail!("{}", response.message);
    }
    println!("{}", response.message);
    Ok(())
}

pub(crate) async fn handle_hook() -> Result<()> {
    let mut input = Vec::new();
    tokio::io::stdin()
        .take(1024 * 1024)
        .read_to_end(&mut input)
        .await?;
    let event = global_claude::InteractiveStart::parse(&input)?;
    let response = send(ControlCommand::Started {
        session_start: event.hook_event_name == "SessionStart",
        session_id: event.session_id,
        cwd: event.cwd,
        model: event.model,
    })
    .await?;
    if !response.ok {
        bail!("{}", response.message);
    }
    Ok(())
}

async fn send(command: ControlCommand) -> Result<ControlResponse> {
    let socket = std::env::var(SOCKET_ENV).context("this Claude session has no Mando supervisor. Exit Claude and relaunch from the same directory with `mando claude -- --resume <exact-session-id>`; subsequent `!mando switch` commands will work")?;
    let request = ControlRequest {
        secret: std::env::var(SECRET_ENV).context("missing Mando launcher authentication")?,
        generation: std::env::var(GENERATION_ENV)
            .context("missing Mando Claude process identity")?,
        command,
    };
    tokio::time::timeout(std::time::Duration::from_secs(45), async {
        let mut stream = UnixStream::connect(&socket).await.context(
            "Mando launcher is no longer running; resume this conversation through `mando claude`",
        )?;
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        read_message(&mut stream).await
    })
    .await
    .context("Mando profile switch timed out; the current Claude process remains running")?
}

pub(crate) async fn read_message<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T> {
    let mut line = String::new();
    BufReader::new(stream.take(64 * 1024))
        .read_line(&mut line)
        .await?;
    serde_json::from_str(&line).context("invalid Mando launcher control message")
}

pub(crate) async fn respond(stream: &mut UnixStream, response: ControlResponse) -> Result<()> {
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.shutdown().await?;
    Ok(())
}
