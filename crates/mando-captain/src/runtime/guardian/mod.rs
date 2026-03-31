//! Self-improve guardian — monitors logs and auto-triggers repairs in
//! isolated worktrees with write-ahead intent logging.

pub mod intent;
pub mod signature;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use serde::Serialize;
use tracing::{info, warn};

use crate::io::git;
use intent::{clear_intent, write_intent, Intent, IntentStatus};
use mando_cc::{CcConfig, CcOneShot};
use mando_types::now_rfc3339;
use signature::incident_signature;

/// Result of a `trigger_once` call, returned to callers.
#[derive(Debug, Serialize)]
pub struct TriggerResult {
    pub triggered: bool,
    pub skipped_reason: Option<String>,
    pub incidents: Vec<String>,
    pub repair_output: Option<String>,
}

/// Cooldown state persisted to disk — per-signature dedup.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct CooldownState {
    last_trigger_by_sig: HashMap<String, f64>,
    repair_timestamps: Vec<f64>,
}

/// Self-improve guardian with worktree isolation and intent logging.
pub struct SelfImproveGuardian {
    config: Config,
    workflow: CaptainWorkflow,
    state_dir: PathBuf,
    cooldown_path: PathBuf,
    cooldown: CooldownState,
    last_trigger_by_sig_mem: HashMap<String, Instant>,
    repair_in_progress: bool,
    pool: sqlx::SqlitePool,
}

impl SelfImproveGuardian {
    pub fn new(config: Config, workflow: CaptainWorkflow, pool: sqlx::SqlitePool) -> Self {
        let state_dir = mando_config::state_dir().join("self-improve");
        let cooldown_path = state_dir.join("cooldown.json");
        let cooldown = match std::fs::read_to_string(&cooldown_path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(
                    module = "guardian",
                    error = %e,
                    "cooldown state corrupt — resetting cooldowns"
                );
                CooldownState::default()
            }),
            Err(e) => {
                tracing::debug!(module = "guardian", error = %e, "cooldown file unreadable — using defaults");
                CooldownState::default()
            }
        };

        Self {
            config,
            workflow,
            state_dir,
            cooldown_path,
            cooldown,
            last_trigger_by_sig_mem: HashMap::new(),
            repair_in_progress: false,
            pool,
        }
    }

    /// One-shot trigger: scan logs (or use provided text), run repair, return structured result.
    ///
    /// Feature gating (`features.dev_mode`) is enforced at the route/caller level,
    /// not here — the guardian assumes the caller already checked the gate.
    pub async fn trigger_once(&mut self, text: Option<&str>) -> Result<TriggerResult> {
        let (incident_text, incidents) = match text {
            Some(t) if !t.is_empty() => (t.to_string(), vec![]),
            _ => {
                let found = self.scan_logs();
                if found.is_empty() {
                    return Ok(TriggerResult {
                        triggered: false,
                        skipped_reason: Some("no errors found in logs".into()),
                        incidents: vec![],
                        repair_output: None,
                    });
                }
                (found.join("\n"), found)
            }
        };

        match self.trigger(&incident_text).await {
            Ok(true) => Ok(TriggerResult {
                triggered: true,
                skipped_reason: None,
                incidents,
                repair_output: Some("repair completed".into()),
            }),
            Ok(false) => Ok(TriggerResult {
                triggered: false,
                skipped_reason: Some("trigger gated or no changes".into()),
                incidents,
                repair_output: None,
            }),
            Err(e) => Ok(TriggerResult {
                triggered: true,
                skipped_reason: None,
                incidents,
                repair_output: Some(format!("repair failed: {e}")),
            }),
        }
    }

    /// Scan log files for error patterns.
    pub(crate) fn scan_logs(&self) -> Vec<String> {
        let si = &self.config.tools.cc_self_improve;
        let mut incidents = Vec::new();
        for log_path in &si.log_paths {
            let path = mando_config::expand_tilde(log_path);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(module = "guardian", error = %e, path = %path.display(), "cannot read log file");
                    continue;
                }
            };
            let lines: Vec<&str> = content.lines().rev().take(100).collect();
            for line in &lines {
                for pattern in &si.error_patterns {
                    if line.contains(pattern.as_str()) {
                        let should_ignore = si
                            .ignore_patterns
                            .iter()
                            .any(|ip| line.contains(ip.as_str()));
                        if !should_ignore {
                            incidents.push(line.to_string());
                        }
                    }
                }
            }
        }
        incidents
    }

    /// Trigger a repair for an incident. Returns Ok(true) if repair ran.
    pub(crate) async fn trigger(&mut self, incident_text: &str) -> Result<bool> {
        if self.repair_in_progress {
            info!(module = "guardian", "skipped, repair in progress");
            return Ok(false);
        }

        let si = &self.config.tools.cc_self_improve;
        let sig = incident_signature(incident_text);
        if let Some(reason) = self.gate_incident(&sig, si.cooldown_s, si.max_repairs_per_hour) {
            info!(module = "guardian", reason = %reason, "skipped");
            return Ok(false);
        }

        self.repair_in_progress = true;
        let result = self.run_repair(incident_text, &sig).await;
        self.repair_in_progress = false;
        result
    }

    fn gate_incident(&mut self, sig: &str, cooldown_s: u64, max_per_hour: u32) -> Option<String> {
        let now = Instant::now();

        // Per-signature cooldown.
        if let Some(last) = self.last_trigger_by_sig_mem.get(sig) {
            if now.duration_since(*last) < Duration::from_secs(cooldown_s) {
                return Some(format!(
                    "cooldown active for sig {}",
                    &sig[..8.min(sig.len())]
                ));
            }
        }

        // Hourly rate limit.
        let now_epoch = epoch_secs();
        let one_hour_ago = now_epoch - 3600.0;
        self.cooldown
            .repair_timestamps
            .retain(|ts| *ts >= one_hour_ago);
        if self.cooldown.repair_timestamps.len() >= max_per_hour as usize {
            return Some("hourly repair budget exhausted".into());
        }

        // Record this trigger.
        self.last_trigger_by_sig_mem.insert(sig.to_string(), now);
        self.cooldown
            .last_trigger_by_sig
            .insert(sig.to_string(), now_epoch);
        self.cooldown.repair_timestamps.push(now_epoch);
        self.save_cooldown();
        None
    }

    async fn run_repair(&mut self, incident_text: &str, sig: &str) -> Result<bool> {
        let si = &self.config.tools.cc_self_improve;
        let repo_path = self.repo_path();
        let now = now_rfc3339();

        info!(
            module = "guardian",
            sig = %sig,
            msg = %&incident_text[..incident_text.len().min(200)],
            "triggering repair"
        );

        // Create isolated worktree.
        let stamp = now_rfc3339().replace([':', '-', '+'], "").replace('T', "-")[..15].to_string();
        let branch = format!("self-improve/{}-{}", stamp, &sig[..sig.len().min(8)]);
        let wt = git::worktree_path(&repo_path, &format!("self-improve-{stamp}"));
        let default_ref = git::default_branch(&repo_path).await?;

        if let Err(e) = git::delete_local_branch(&repo_path, &branch).await {
            // Branch may not exist yet — this is expected on first run.
            tracing::debug!(module = "guardian", branch = %branch, error = %e, "pre-cleanup branch delete failed (may not exist)");
        }
        git::create_worktree(&repo_path, &branch, &wt, &default_ref).await?;

        // Write-ahead intent: CC starting.
        let incident_json = serde_json::json!({
            "source": "trigger",
            "message": incident_text,
            "detected_at": &now,
            "signature": sig,
        });

        std::fs::create_dir_all(&self.state_dir)?;
        write_intent(
            &self.state_dir,
            &Intent {
                wt_path: wt.display().to_string(),
                branch: branch.clone(),
                incident: incident_json.clone(),
                status: IntentStatus::CcRunning,
                updated_at: now.clone(),
            },
        )?;

        // Run headless CC in the worktree.
        let check_cmd = self.resolve_check_command();
        let mut vars = HashMap::new();
        vars.insert("source", "trigger");
        vars.insert("detected_at", now.as_str());
        vars.insert("message", incident_text);
        vars.insert("check_command", check_cmd.as_str());
        let prompt = mando_config::render_prompt("guardian_repair", &self.workflow.prompts, &vars)
            .map_err(|e| anyhow::anyhow!(e))?;

        let cc_result = CcOneShot::run(
            &prompt,
            CcConfig::builder()
                .model(&si.model)
                .cwd(wt.clone())
                .timeout(Duration::from_secs(si.timeout_s.max(60)))
                .caller("guardian-repair")
                .build(),
        )
        .await;

        match cc_result {
            Err(e) => {
                warn!(module = "guardian", error = %e, "CC failed");
                clear_intent(&self.state_dir);
                self.cleanup_worktree(&wt, &branch, &repo_path).await;
                return Ok(false);
            }
            Ok(ref result) => {
                crate::io::headless_cc::log_cc_session(
                    &self.pool,
                    &crate::io::headless_cc::SessionLogEntry {
                        session_id: &result.session_id,
                        cwd: &wt,
                        model: &si.model,
                        caller: "guardian-repair",
                        cost_usd: result.cost_usd,
                        duration_ms: result.duration_ms,
                        resumed: false,
                        task_id: "",
                        status: mando_types::SessionStatus::Stopped,
                        worker_name: "",
                    },
                )
                .await;
                // Update intent: CC finished.
                write_intent(
                    &self.state_dir,
                    &Intent {
                        wt_path: wt.display().to_string(),
                        branch: branch.clone(),
                        incident: incident_json,
                        status: IntentStatus::CcDone,
                        updated_at: now_rfc3339(),
                    },
                )?;
            }
        }

        // Check for changes.
        let has_changes = git::has_changes(&wt).await.unwrap_or(false);
        if !has_changes {
            info!(module = "guardian", "CC made no changes");
            clear_intent(&self.state_dir);
            self.cleanup_worktree(&wt, &branch, &repo_path).await;
            return Ok(true);
        }

        // Merge to main.
        info!(module = "guardian", branch = %branch, "merging repair branch to main");
        match git::rebase_and_push_to_main(&wt, &repo_path).await {
            Ok(()) => info!(module = "guardian", "repair pushed to main"),
            Err(e) => warn!(module = "guardian", error = %e, "merge failed"),
        }

        clear_intent(&self.state_dir);
        self.cleanup_worktree(&wt, &branch, &repo_path).await;
        Ok(true)
    }

    fn resolve_check_command(&self) -> String {
        let fallback = "the project's quality gate (formatting, linting, tests — check CLAUDE.md for the exact command)";
        let repo = self.repo_path();
        let repo_str = repo.to_string_lossy();
        for pc in self.config.captain.projects.values() {
            let pc_path = mando_config::expand_tilde(&pc.path);
            if pc_path == repo || repo_str == pc.path {
                if pc.check_command.is_empty() {
                    return fallback.to_string();
                }
                return format!("`{}`", pc.check_command);
            }
        }
        fallback.to_string()
    }

    fn repo_path(&self) -> PathBuf {
        let cwd = &self.config.tools.cc_self_improve.cwd;
        if cwd.is_empty() {
            self.config
                .captain
                .projects
                .values()
                .next()
                .map(|rc| mando_config::expand_tilde(&rc.path))
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            mando_config::expand_tilde(cwd)
        }
    }

    async fn cleanup_worktree(
        &self,
        wt_path: &std::path::Path,
        branch: &str,
        repo_path: &std::path::Path,
    ) {
        if let Err(e) = git::remove_worktree(repo_path, wt_path).await {
            warn!(
                module = "guardian",
                path = %wt_path.display(),
                error = %e,
                "failed to remove worktree — disk space leak"
            );
        }
        if let Err(e) = git::delete_local_branch(repo_path, branch).await {
            warn!(
                module = "guardian",
                branch = %branch,
                error = %e,
                "failed to delete local branch"
            );
        }
    }

    fn save_cooldown(&self) {
        if let Some(parent) = self.cooldown_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(module = "guardian", error = %e, "failed to create cooldown dir");
                return;
            }
        }
        let json = match serde_json::to_string(&self.cooldown) {
            Ok(j) => j,
            Err(e) => {
                warn!(module = "guardian", error = %e, "failed to serialize cooldown state");
                return;
            }
        };
        if let Err(e) = std::fs::write(&self.cooldown_path, &json) {
            warn!(
                module = "guardian",
                error = %e,
                "failed to save cooldown state — may trigger duplicate repairs after restart"
            );
        }
    }
}

fn epoch_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
