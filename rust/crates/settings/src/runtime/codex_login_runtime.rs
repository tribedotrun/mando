//! Codex "sign in with browser" background flow.
//!
//! Spawns `codex login`, tracks the single in-flight (or most-recently
//! finished) flow on [`SettingsRuntime`], and — once the browser OAuth
//! round-trip completes — feeds the captured `auth.json` through the
//! existing [`SettingsRuntime::store_codex_credential`] pipeline. Mirrors
//! the panic-safety pattern used by scout's research runs
//! (`scout::runtime::daemon_research_runtime`): the background task is
//! wrapped in `catch_unwind` so a panic marks the flow `Failed` instead of
//! crashing the daemon.
//!
//! The pure decision layer (capture ownership, row-scoped guards, label
//! resolution, failure messages) lives in `codex_login_rules.rs`.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use api_types::{CodexCredentialAddWarning, CodexLoginFlowInfo, CodexLoginStatus};

use crate::io::codex_credentials;
use crate::io::codex_login::{self, CodexLoginError};

use super::codex_credentials_runtime::CodexCredentialError;
use super::codex_login_rules::{
    captured_account_mismatches, captured_identity, codex_login_store_error_message,
    flow_owns_capture, panic_to_string, resolve_codex_login_label, target_row_intact,
    ROW_LOGIN_ACCOUNT_MISMATCH, ROW_LOGIN_TARGET_CHANGED,
};
use super::settings_runtime::SettingsRuntime;

/// Single in-flight (or most-recently-finished) Codex browser login flow.
/// `SettingsRuntime` holds at most one at a time behind
/// `Arc<Mutex<Option<CodexLoginFlow>>>` — starting a new flow cancels and
/// replaces whatever is there, and the background task mutates this same
/// slot as it progresses so the status endpoint always reads live state.
pub struct CodexLoginFlow {
    login_id: String,
    status: CodexLoginStatus,
    credential_id: Option<i64>,
    auth_url: Option<String>,
    label: Option<String>,
    warning: Option<CodexCredentialAddWarning>,
    error: Option<String>,
    cancel: CancellationToken,
}

impl CodexLoginFlow {
    fn to_wire(&self) -> CodexLoginFlowInfo {
        CodexLoginFlowInfo {
            login_id: self.login_id.clone(),
            status: self.status,
            credential_id: self.credential_id,
            auth_url: self.auth_url.clone(),
            label: self.label.clone(),
            warning: self.warning.clone(),
            error: self.error.clone(),
        }
    }
}

/// Result of requesting a new login flow: the caller polls
/// `codex_login_status` with this id.
pub struct StartedCodexLogin {
    pub login_id: String,
}

/// Row-scoped re-login target loaded up front from the credential row named
/// by `StartCodexLoginRequest.credential_id`. The captured session must
/// belong to `account_id`, and `label` wins label resolution (branch 0).
/// `id` is re-checked just before storing so a row deleted mid-flow is not
/// resurrected by the store's account-keyed upsert.
#[derive(Clone)]
struct LoginTarget {
    id: i64,
    account_id: String,
    label: String,
}

impl SettingsRuntime {
    /// Cancel any pending flow, start a fresh `codex login`, and return
    /// immediately with the new flow's id. The browser OAuth round-trip,
    /// `auth.json` capture, and credential store all happen on a tracked
    /// background task so the HTTP response returns right away.
    ///
    /// When `credential_id` is set (row-scoped re-login), the target row is
    /// validated up front — before anything spawns — so a missing or
    /// non-Codex row surfaces as an immediate error: `NotFound` for a
    /// missing id, `NotCodex` for a Claude row, `NoAccountId` for a Codex
    /// row without a stored account.
    #[tracing::instrument(skip_all)]
    pub async fn start_codex_login(
        &self,
        label: Option<String>,
        credential_id: Option<i64>,
        bus: Arc<global_bus::EventBus>,
        tracker: &TaskTracker,
    ) -> Result<StartedCodexLogin, CodexCredentialError> {
        let target = match credential_id {
            Some(id) => {
                let row = crate::io::credentials::get_row_by_id(&self.db_pool, id)
                    .await?
                    .ok_or(CodexCredentialError::NotFound(id))?;
                if row.provider != "codex" {
                    return Err(CodexCredentialError::NotCodex);
                }
                let account_id = row.account_id.ok_or(CodexCredentialError::NoAccountId)?;
                Some(LoginTarget {
                    id,
                    account_id,
                    label: row.label,
                })
            }
            None => None,
        };

        codex_login::prune_stale_login_homes();

        let login_id = global_infra::uuid::Uuid::v4().to_string();
        let cancel = CancellationToken::new();
        let requested_label = label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string);
        // Row-scoped flows surface the target row's label while Pending —
        // it is also what the credential keeps on Success (label branch 0).
        let initial_label = target
            .as_ref()
            .map(|t| t.label.clone())
            .or_else(|| requested_label.clone());
        {
            let mut slot = self.codex_login.lock().await;
            if let Some(existing) = slot.as_mut() {
                if existing.status == CodexLoginStatus::Pending {
                    existing.cancel.cancel();
                }
            }
            *slot = Some(CodexLoginFlow {
                login_id: login_id.clone(),
                status: CodexLoginStatus::Pending,
                credential_id,
                auth_url: None,
                label: initial_label,
                warning: None,
                error: None,
                cancel: cancel.clone(),
            });
        }

        let runtime = self.clone();
        let task_login_id = login_id.clone();
        tracker.spawn(async move {
            let result = AssertUnwindSafe(runtime.run_codex_login_flow(
                task_login_id.clone(),
                requested_label,
                target,
                cancel,
            ))
            .catch_unwind()
            .await;
            if let Err(panic) = result {
                let msg = panic_to_string(&panic);
                tracing::error!(
                    module = "settings-runtime-codex_login_runtime",
                    login_id = %task_login_id,
                    panic = %msg,
                    "codex login flow panicked"
                );
                runtime
                    .finish_codex_login(
                        &task_login_id,
                        CodexLoginStatus::Failed,
                        None,
                        None,
                        Some(msg),
                    )
                    .await;
            }
            bus.send(global_bus::BusPayload::Credentials(None));
        });

        Ok(StartedCodexLogin { login_id })
    }

    /// Current flow snapshot, or `None` when no flow has run since the
    /// daemon started.
    #[tracing::instrument(skip_all)]
    pub async fn codex_login_status(&self) -> Option<CodexLoginFlowInfo> {
        self.codex_login
            .lock()
            .await
            .as_ref()
            .map(CodexLoginFlow::to_wire)
    }

    /// Cancel the pending flow, if any. Returns `true` when a `Pending`
    /// flow was actually cancelled; the background task performs the kill
    /// and the transition to `Cancelled`.
    #[tracing::instrument(skip_all)]
    pub async fn cancel_codex_login(&self) -> bool {
        let mut slot = self.codex_login.lock().await;
        match slot.as_mut() {
            Some(flow) if flow.status == CodexLoginStatus::Pending => {
                flow.cancel.cancel();
                true
            }
            _ => false,
        }
    }

    async fn run_codex_login_flow(
        &self,
        login_id: String,
        requested_label: Option<String>,
        target: Option<LoginTarget>,
        cancel: CancellationToken,
    ) {
        let auth_url_runtime = self.clone();
        let auth_url_login_id = login_id.clone();
        let on_auth_url = move |url: String| {
            let runtime = auth_url_runtime.clone();
            let login_id = auth_url_login_id.clone();
            tokio::spawn(async move {
                runtime.set_codex_login_auth_url(&login_id, url).await;
            });
        };

        match codex_login::run_codex_login(on_auth_url, cancel).await {
            Ok(capture) => {
                self.complete_codex_login(&login_id, requested_label, target, capture.auth_json)
                    .await;
            }
            Err(CodexLoginError::Cancelled) => {
                self.finish_codex_login(&login_id, CodexLoginStatus::Cancelled, None, None, None)
                    .await;
            }
            Err(err) => {
                self.finish_codex_login(
                    &login_id,
                    CodexLoginStatus::Failed,
                    None,
                    None,
                    Some(err.to_string()),
                )
                .await;
            }
        }
    }

    async fn set_codex_login_auth_url(&self, login_id: &str, url: String) {
        let mut slot = self.codex_login.lock().await;
        if let Some(flow) = slot.as_mut() {
            if flow.login_id == login_id {
                flow.auth_url = Some(url);
            }
        }
    }

    async fn finish_codex_login(
        &self,
        login_id: &str,
        status: CodexLoginStatus,
        label: Option<String>,
        warning: Option<CodexCredentialAddWarning>,
        error: Option<String>,
    ) {
        let mut slot = self.codex_login.lock().await;
        if let Some(flow) = slot.as_mut() {
            if flow.login_id == login_id {
                flow.status = status;
                if label.is_some() {
                    flow.label = label;
                }
                flow.warning = warning;
                flow.error = error;
            }
        }
    }

    async fn complete_codex_login(
        &self,
        login_id: &str,
        requested_label: Option<String>,
        target: Option<LoginTarget>,
        auth_json: String,
    ) {
        // Ownership check before ANY side effect: a cancelled-and-replaced
        // flow's child can still exit successfully around the cancel, and
        // the login_id guard inside finish_codex_login only protects the
        // UI state update, not the store. Check under the slot lock, then
        // release it before the (network-bound) store below — holding the
        // lock across the store would block status/cancel/start for the
        // whole force-refresh + probe. A replacement landing between this
        // check and the store is re-guarded by finish_codex_login's
        // login_id check; that residual window is milliseconds wide,
        // versus the minutes-wide browser-flow window this check closes.
        {
            let slot = self.codex_login.lock().await;
            let owned = flow_owns_capture(
                slot.as_ref()
                    .map(|flow| (flow.login_id.as_str(), flow.status)),
                login_id,
            );
            if !owned {
                tracing::info!(
                    module = "settings-runtime-codex_login_runtime",
                    login_id = %login_id,
                    "dropping codex login capture for an abandoned flow"
                );
                return;
            }
        }

        let identity = match captured_identity(&auth_json) {
            Ok(identity) => identity,
            Err(err) => {
                self.finish_codex_login(
                    login_id,
                    CodexLoginStatus::Failed,
                    None,
                    None,
                    Some(codex_login_store_error_message(&err)),
                )
                .await;
                return;
            }
        };

        // Row-scoped guard: the captured session must belong to the target
        // row's account. On mismatch the flow fails WITHOUT storing — no
        // force-refresh has happened yet, so the orphaned capture stays
        // untouched server-side.
        if captured_account_mismatches(
            target.as_ref().map(|t| t.account_id.as_str()),
            &identity.account_id,
        ) {
            self.finish_codex_login(
                login_id,
                CodexLoginStatus::Failed,
                None,
                None,
                Some(ROW_LOGIN_ACCOUNT_MISMATCH.to_string()),
            )
            .await;
            return;
        }

        let existing_label = match existing_codex_label(&self.db_pool, &identity.account_id).await {
            Ok(label) => label,
            Err(err) => {
                self.finish_codex_login(
                    login_id,
                    CodexLoginStatus::Failed,
                    None,
                    None,
                    Some(codex_login_store_error_message(&err)),
                )
                .await;
                return;
            }
        };

        let resolved_label = resolve_codex_login_label(
            target.as_ref().map(|t| t.label.as_str()),
            requested_label.as_deref(),
            existing_label.as_deref(),
            identity.email.as_deref(),
            &identity.account_id,
        );

        // Row-scoped re-check just before storing: if the user deleted (or
        // otherwise changed) the target credential while the browser flow
        // was pending, store_codex_credential's account-keyed upsert would
        // INSERT a fresh row under the old label and resurrect the deletion.
        if let Some(target) = &target {
            let row = match crate::io::credentials::get_row_by_id(&self.db_pool, target.id).await {
                Ok(row) => row,
                Err(err) => {
                    self.finish_codex_login(
                        login_id,
                        CodexLoginStatus::Failed,
                        None,
                        None,
                        Some(codex_login_store_error_message(&CodexCredentialError::Db(
                            err,
                        ))),
                    )
                    .await;
                    return;
                }
            };
            let intact = target_row_intact(
                row.as_ref()
                    .map(|row| (row.provider.as_str(), row.account_id.as_deref())),
                &identity.account_id,
            );
            if !intact {
                self.finish_codex_login(
                    login_id,
                    CodexLoginStatus::Failed,
                    None,
                    None,
                    Some(ROW_LOGIN_TARGET_CHANGED.to_string()),
                )
                .await;
                return;
            }
        }

        match self
            .store_codex_credential(&resolved_label, &auth_json)
            .await
        {
            Ok(stored) => {
                self.finish_codex_login(
                    login_id,
                    CodexLoginStatus::Success,
                    Some(resolved_label),
                    stored.warning,
                    None,
                )
                .await;
            }
            Err(err) => {
                self.finish_codex_login(
                    login_id,
                    CodexLoginStatus::Failed,
                    None,
                    None,
                    Some(codex_login_store_error_message(&err)),
                )
                .await;
            }
        }
    }
}

/// Label an already-stored credential row for this account carries, if any
/// (label resolution branch 2 — an unscoped re-login keeps that label).
async fn existing_codex_label(
    db_pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<Option<String>, CodexCredentialError> {
    let existing_id = codex_credentials::find_codex_id_by_account(db_pool, account_id).await?;
    match existing_id {
        Some(id) => Ok(crate::io::credentials::get_row_by_id(db_pool, id)
            .await?
            .map(|row| row.label)),
        None => Ok(None),
    }
}
