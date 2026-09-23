//! Terminal-preserving Claude supervisor and managed-account handoff.
use std::io::{IsTerminal, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use api_types::{
    CredentialLeaseHeartbeatRequest, CredentialLeaseReleaseRequest, CredentialPick,
    CredentialRouteRequest,
};
use clap::Args;
use tokio::net::{UnixListener, UnixStream};

use crate::claude_control::{self, ControlCommand, ControlRequest, ControlResponse};
use crate::http::DaemonClient;

#[derive(Args)]
pub(crate) struct ClaudeArgs {
    /// Initial saved Mando account label (or numeric credential id).
    #[arg(short = 'a', long, visible_aliases = ["label", "id"])]
    account: Option<String>,
    /// Claude interactive options, following --.
    #[arg(last = true)]
    args: Vec<String>,
}

struct Running {
    pick: CredentialPick,
    session_id: Option<String>,
    cwd: String,
    model: Option<String>,
    pid: u32,
    generation: String,
    started: bool,
    confirmed: bool,
}

struct Pending {
    label: String,
    reason: String,
}

struct Supervisor {
    client: DaemonClient,
    launch_id: String,
    secret: String,
    pending: Option<Pending>,
    discard_reservation: bool,
}

fn random_id() -> Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) async fn handle(args: ClaudeArgs) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("mando claude requires an interactive terminal; use claude directly for print/pipe mode");
    }
    if std::env::var_os(claude_control::SOCKET_ENV).is_some() {
        bail!(
            "already inside a supervised Claude session; use `mando switch` to change its profile"
        );
    }
    let executable = std::env::current_exe()?;
    let launch = global_claude::InteractiveLaunch::prepare(
        &args.args,
        &global_claude::hook_command(&executable)?,
    )?;
    let directory = tempfile::Builder::new()
        .prefix("mando-claude-")
        .tempdir_in("/tmp")?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let socket_path = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket_path)?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    let settings_path = global_claude::hook_settings_path(directory.path());
    launch.write_settings(&settings_path)?;
    let client = DaemonClient::discover_with_timeout(Duration::from_secs(35))?;
    let profile = match args.account {
        Some(account) if account.chars().all(|ch| ch.is_ascii_digit()) && !account.is_empty() => {
            let id: i64 = account.parse().context("invalid credential id")?;
            let credentials = client.get_credentials().await?;
            Some(
                credentials
                    .credentials
                    .into_iter()
                    .find(|credential| {
                        credential.id == id
                            && credential.provider == api_types::CredentialProvider::Claude
                    })
                    .context("Claude credential id not found")?
                    .label,
            )
        }
        other => other,
    };
    let mut supervisor = Supervisor {
        client,
        launch_id: random_id()?,
        secret: random_id()?,
        pending: None,
        discard_reservation: false,
    };
    let routed = supervisor
        .client
        .post_credentials_route(&CredentialRouteRequest {
            launch_id: supervisor.launch_id.clone(),
            current_credential_id: None,
            profile,
            model: launch.model.clone(),
            dry_run: false,
            min_headroom_percent: None,
        })
        .await?;
    let pick = routed
        .pick
        .with_context(|| format!("no usable managed Claude profile: {}", routed.reason))?;
    eprintln!("mando: {}", routed.reason);
    eprintln!("mando: inside Claude use !mando switch [profile], then /exit to continue on the selected account.");
    let result = supervisor
        .run(launch, pick, &settings_path, &socket_path, listener)
        .await;
    let release = supervisor
        .client
        .post_credentials_leases_release(&CredentialLeaseReleaseRequest {
            launch_id: supervisor.launch_id.clone(),
        })
        .await;
    if let Err(error) = release {
        tracing::warn!(error = %error, "failed to release Claude launch lease; it will expire");
    }
    result
}

impl Supervisor {
    async fn run(
        &mut self,
        launch: global_claude::InteractiveLaunch,
        mut pick: CredentialPick,
        settings_path: &std::path::Path,
        socket_path: &std::path::Path,
        listener: UnixListener,
    ) -> Result<()> {
        let mut resume: Option<String> = None;
        let mut current_model = launch.model.clone();
        let mut cwd = std::env::current_dir()?.to_string_lossy().into_owned();
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        loop {
            let generation = random_id()?;
            let mut command = launch.command(
                settings_path,
                resume.as_deref(),
                current_model.as_deref(),
                &pick.token,
                &pick.label,
                pick.id,
            );
            command
                .current_dir(&cwd)
                .env(claude_control::SOCKET_ENV, socket_path)
                .env(claude_control::SECRET_ENV, &self.secret)
                .env(claude_control::GENERATION_ENV, &generation)
                .env("MANDO_CLAUDE_LAUNCH_ID", &self.launch_id);
            // Resolve this checkout's CLI in shell mode, without changing the user's PATH.
            let executable = std::env::current_exe()?;
            if let Some(parent) = executable.parent() {
                let mut path = vec![parent.to_path_buf()];
                if let Some(existing) = std::env::var_os("PATH") {
                    path.extend(std::env::split_paths(&existing));
                }
                command.env("PATH", std::env::join_paths(path)?);
            }
            let mut child = command
                .spawn()
                .context("start Claude interactive process")?;
            let pid = child.id().context("Claude child has no process id")?;
            let mut running = Running {
                pick,
                session_id: resume.clone(),
                cwd: cwd.clone(),
                model: current_model.clone(),
                pid,
                generation,
                started: false,
                confirmed: false,
            };
            let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let status = loop {
                tokio::select! {
                    status = child.wait() => break status.context("wait for Claude")?,
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted?;
                        if let Err(error) = self.control(&mut stream, &mut running).await {
                            tracing::warn!(error = %error, "Claude launcher request failed");
                            if let Err(write_error) = claude_control::respond(&mut stream, ControlResponse { ok: false, message: format!("{error:#}") }).await {
                                tracing::warn!(error = %write_error, "failed to return Claude launcher error");
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        if running.started {
                            match self.heartbeat(&running, !running.confirmed).await {
                                Ok(()) => running.confirmed = true,
                                Err(error) => tracing::warn!(error = %error, "Claude launch heartbeat failed"),
                            }
                        }
                    }
                    signal = tokio::signal::ctrl_c() => {
                        // Terminal SIGINT reaches Claude too. Keep its supervisor alive while
                        // Claude handles cancellation / its double-Ctrl-C exit gesture.
                        signal.context("listen for terminal interrupt")?;
                    }
                    _ = terminate.recv() => {
                        child.kill().await.context("terminate Claude with its supervisor")?;
                        bail!("Claude supervisor terminated; conversation can be resumed");
                    }
                }
            };
            let Some(pending) = self.pending.take() else {
                if !status.success() {
                    bail!("Claude exited with {status}");
                }
                return Ok(());
            };
            let session_id = running.session_id.context(
                "Claude never confirmed its session; refusing to guess a conversation to resume",
            )?;
            if !status.success() {
                bail!("Claude exited with {status}; switch canceled. Resume {session_id} using `mando claude -- --resume {session_id}`");
            }
            // A reservation can expire during sleep or daemon outage. Re-check just before
            // launch, retaining the original account if the destination is unavailable.
            let routed = self
                .client
                .post_credentials_route(&CredentialRouteRequest {
                    launch_id: self.launch_id.clone(),
                    current_credential_id: Some(running.pick.id),
                    profile: Some(pending.label.clone()),
                    model: running.model.clone(),
                    dry_run: false,
                    min_headroom_percent: None,
                })
                .await;
            let previous_id = running.pick.id;
            pick = match routed {
                Ok(response) => {
                    match response.pick {
                        Some(target) => {
                            eprintln!("mando: switching to '{}'; {}", target.label, pending.reason);
                            target
                        }
                        None => {
                            eprintln!("mando: destination unavailable ({}); resuming original profile '{}'", response.reason, running.pick.label);
                            running.pick
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "Claude switch revalidation failed");
                    eprintln!(
                        "mando: could not revalidate destination; resuming original profile '{}'",
                        running.pick.label
                    );
                    running.pick
                }
            };
            if pick.id == previous_id {
                self.discard_reservation = true;
            }
            resume = Some(session_id);
            cwd = running.cwd;
            current_model = running.model;
        }
    }

    async fn heartbeat(&mut self, running: &Running, confirm: bool) -> Result<()> {
        // A fallback must not keep renewing the abandoned destination's reservation.
        // Retry cleanup before any live renewal if a daemon outage interrupted it.
        if self.discard_reservation {
            tokio::time::timeout(
                Duration::from_secs(5),
                self.client
                    .post_credentials_leases_release(&CredentialLeaseReleaseRequest {
                        launch_id: self.launch_id.clone(),
                    }),
            )
            .await
            .context("clear abandoned Claude reservation timed out")??;
            self.discard_reservation = false;
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            self.client
                .post_credentials_leases_heartbeat(&CredentialLeaseHeartbeatRequest {
                    launch_id: self.launch_id.clone(),
                    credential_id: running.pick.id,
                    session_id: running.session_id.clone(),
                    pid: running.pid,
                    cwd: running.cwd.clone(),
                    model: running.model.clone(),
                    confirm_reservation: confirm,
                }),
        )
        .await
        .context("Claude lease heartbeat timed out")??;
        Ok(())
    }

    async fn control(&mut self, stream: &mut UnixStream, running: &mut Running) -> Result<()> {
        let request: ControlRequest =
            tokio::time::timeout(Duration::from_secs(3), claude_control::read_message(stream))
                .await
                .context("control request timed out")??;
        if request.secret != self.secret || request.generation != running.generation {
            bail!("stale or unauthenticated Claude launcher request");
        }
        let message = match request.command {
            ControlCommand::Started {
                session_start,
                session_id,
                cwd,
                model,
            } => {
                if !session_start && running.session_id.as_ref() != Some(&session_id) {
                    bail!("ignoring hook from a different Claude conversation");
                }
                if !PathBuf::from(&cwd).is_absolute() {
                    bail!("Claude startup directory must be absolute");
                }
                running.session_id = Some(session_id);
                running.cwd = cwd;
                if model.is_some() {
                    running.model = model;
                }
                if session_start {
                    running.started = true;
                    running.confirmed = false;
                }
                match self.heartbeat(running, !running.confirmed).await {
                    Ok(()) => running.confirmed = true,
                    Err(error) => {
                        tracing::warn!(error = %error, "Claude started but lease confirmation failed; heartbeat will retry")
                    }
                }
                String::new()
            }
            ControlCommand::Switch { profile, dry_run } => {
                if !running.started {
                    bail!("Claude startup has not confirmed its session yet; wait for the prompt before switching");
                }
                if profile.as_deref() == Some(running.pick.label.as_str()) {
                    let message = if dry_run {
                        format!(
                            "Already using '{}'; no profile change requested.",
                            running.pick.label
                        )
                    } else {
                        self.client
                            .post_credentials_leases_release(&CredentialLeaseReleaseRequest {
                                launch_id: self.launch_id.clone(),
                            })
                            .await?;
                        self.pending = None;
                        if let Err(error) = self.heartbeat(running, false).await {
                            tracing::warn!(error = %error, "failed to renew lease after canceling switch");
                        }
                        format!(
                            "Queued switch canceled. Staying on '{}'.",
                            running.pick.label
                        )
                    };
                    return claude_control::respond(stream, ControlResponse { ok: true, message })
                        .await;
                }
                let response = self
                    .client
                    .post_credentials_route(&CredentialRouteRequest {
                        launch_id: self.launch_id.clone(),
                        current_credential_id: Some(running.pick.id),
                        profile,
                        model: running.model.clone(),
                        dry_run,
                        min_headroom_percent: None,
                    })
                    .await
                    .with_context(|| {
                        let previous = self
                            .pending
                            .as_ref()
                            .map(|pending| {
                                format!(" Previous switch to '{}' remains queued.", pending.label)
                            })
                            .unwrap_or_default();
                        format!("profile routing failed.{previous}")
                    })?;
                if dry_run {
                    let mut lines = vec![format!(
                        "Preview: {}. Current profile '{}' remains active.",
                        response.reason, running.pick.label
                    )];
                    for candidate in response.candidates {
                        let headroom = candidate
                            .headroom_percent
                            .map(|value| format!("{value:.1}%"))
                            .unwrap_or_else(|| "unknown".into());
                        lines.push(format!(
                            "  {}: {}; {} live, {} pending; headroom {}",
                            candidate.label,
                            candidate.reason,
                            candidate.live_sessions,
                            candidate.pending_launches,
                            headroom
                        ));
                    }
                    lines.join("\n")
                } else {
                    let pick = response.pick.with_context(|| {
                        let previous = self
                            .pending
                            .as_ref()
                            .map(|pending| {
                                format!(" Previous switch to '{}' remains queued.", pending.label)
                            })
                            .unwrap_or_default();
                        format!(
                            "requested switch unavailable: {}.{previous}",
                            response.reason
                        )
                    })?;
                    let message = format!("Switch to '{}' queued: {}. Type /exit in Claude when ready; Mando will resume this exact conversation. Current profile '{}' remains active until then.", pick.label, response.reason, running.pick.label);
                    self.pending = Some(Pending {
                        label: pick.label,
                        reason: response.reason,
                    });
                    message
                }
            }
        };
        claude_control::respond(stream, ControlResponse { ok: true, message }).await
    }
}
