//! Captain notification delivery — emits `BusPayload::Notification` on the EventBus.
//!
//! Supports edit-in-place: repeated updates for the same task_key carry the
//! key in the payload so SSE consumers can edit their previous message.
//!
//! LOW/NORMAL notifications are batched during a captain tick and flushed as
//! a single "Captain summary" message at tick end. HIGH+ events send immediately.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use api_types::{NotificationKind, NotificationPayload, NotifyLevel};
use global_bus::{BusPayload, EventBus};

/// Tier thresholds for rate limit notifications (percentage points).
const RATE_LIMIT_TIERS: [u32; 5] = [80, 85, 90, 95, 99];

/// 7-day TTL for tier notifications (aligned with Claude's weekly rate limit window).
const TIER_TTL: time::Duration = time::Duration::days(7);

/// Process-level rate-limit tier tracker — persists across captain ticks
/// for the daemon's lifetime. Merge/review tasks spawned from different
/// ticks all share this single instance.
static RATE_LIMIT_TRACKER: LazyLock<Mutex<RateLimitTierTracker>> =
    LazyLock::new(|| Mutex::new(RateLimitTierTracker::default()));

/// Tracks which rate-limit warning tiers have been notified within the
/// current 7-day window.
#[derive(Debug, Default)]
struct RateLimitTierTracker {
    /// tier (80/85/90/95/99) → timestamp when that tier last fired.
    notified: HashMap<u32, time::OffsetDateTime>,
    /// Last seen utilization percentage. A drop means the rate-limit
    /// window rolled over — triggers an implicit clear.
    last_pct: Option<u32>,
}

impl RateLimitTierTracker {
    /// Should a warning at `utilization_pct`% fire a notification?
    /// Returns `Some(tier)` if a new tier was crossed, `None` to suppress.
    fn should_notify_warning(&mut self, utilization_pct: u32) -> Option<u32> {
        // Utilization is cumulative within a window — a drop means the
        // window rolled over. Clear all tiers to start a fresh cycle.
        if let Some(prev) = self.last_pct {
            if utilization_pct < prev {
                self.notified.clear();
            }
        }
        self.last_pct = Some(utilization_pct);

        let now = time::OffsetDateTime::now_utc();
        let tier = *RATE_LIMIT_TIERS
            .iter()
            .rev()
            .find(|&&t| utilization_pct >= t)?;

        if let Some(&notified_at) = self.notified.get(&tier) {
            if now - notified_at < TIER_TTL {
                return None;
            }
        }

        // Mark this tier and all lower tiers so a subsequent drop in
        // utilization doesn't re-alert at a tier we've already passed.
        for &t in &RATE_LIMIT_TIERS {
            if t <= tier {
                self.notified.insert(t, now);
            }
        }
        Some(tier)
    }

    /// Clear all tier state (recovery to allowed).
    fn clear(&mut self) {
        self.notified.clear();
        self.last_pct = None;
    }
}

/// Notification channel handle.
///
/// Emits notification payloads on the EventBus. Messages below the
/// threshold are logged but not emitted. LOW/NORMAL notifications are
/// batched and flushed at tick end via `flush_batch()`.
pub struct Notifier {
    pub threshold: NotifyLevel,
    pub quiet_mode: bool,
    pub notifications_enabled: bool,
    pub repo_slug: Option<String>,
    bus: Arc<EventBus>,
    batch: Mutex<Vec<String>>,
}

impl Notifier {
    /// Clone the inner EventBus handle (cheap Arc clone).
    pub(crate) fn clone_bus(&self) -> Arc<EventBus> {
        Arc::clone(&self.bus)
    }

    /// Create a child notifier that inherits the current delivery settings.
    pub fn fork(&self) -> Self {
        Self {
            threshold: self.threshold,
            quiet_mode: self.quiet_mode,
            notifications_enabled: self.notifications_enabled,
            repo_slug: self.repo_slug.clone(),
            bus: self.clone_bus(),
            batch: Mutex::new(Vec::new()),
        }
    }

    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            threshold: NotifyLevel::Low,
            quiet_mode: false,
            notifications_enabled: true,
            repo_slug: None,
            bus,
            batch: Mutex::new(Vec::new()),
        }
    }

    /// Set the repo slug for PR linkification in notifications.
    pub fn with_repo_slug(mut self, slug: Option<String>) -> Self {
        self.repo_slug = slug;
        self
    }

    /// Toggle whether BusPayload::Notification should be emitted at all.
    pub fn with_notifications_enabled(mut self, enabled: bool) -> Self {
        self.notifications_enabled = enabled;
        self
    }

    /// Send a notification if it meets the threshold and quiet-mode filter.
    #[tracing::instrument(skip_all)]
    pub async fn notify(&self, message: &str, level: NotifyLevel) {
        self.emit(message, level, NotificationKind::Generic, None, None)
            .await;
    }

    /// Send a typed notification with a semantic kind.
    #[tracing::instrument(skip_all)]
    pub async fn notify_typed(
        &self,
        message: &str,
        level: NotifyLevel,
        kind: NotificationKind,
        task_key: Option<&str>,
    ) {
        self.emit(message, level, kind, task_key, None).await;
    }

    async fn emit(
        &self,
        message: &str,
        level: NotifyLevel,
        kind: NotificationKind,
        task_key: Option<&str>,
        reply_markup: Option<api_types::TelegramReplyMarkup>,
    ) {
        if !self.notifications_enabled {
            tracing::debug!(module = "notify", message = %message, "notifications disabled");
            return;
        }

        let quiet = self.quiet_mode;
        if quiet && level < NotifyLevel::High {
            tracing::debug!(module = "notify", message = %message, "suppressed (quiet mode)");
            return;
        }

        if level < self.threshold {
            tracing::debug!(module = "notify", message = %message, "below threshold");
            return;
        }

        // Linkify PR references if repo context is available.
        let final_message = match &self.repo_slug {
            Some(slug) => global_infra::html::linkify_pr_refs(message, slug),
            None => message.to_string(),
        };

        // Batch LOW/NORMAL notifications (no task_key, no buttons) for tick-end summary.
        if level < NotifyLevel::High && task_key.is_none() && reply_markup.is_none() {
            tracing::info!(
                module = "captain-runtime-notify",
                "[notify] batching {:?} notification: {}",
                level,
                message
            );
            match self.batch.lock() {
                Ok(mut batch) => batch.push(final_message),
                Err(e) => {
                    tracing::error!(
                        module = "captain-runtime-notify",
                        "batch mutex poisoned, notification dropped: {e}"
                    );
                }
            }
            return;
        }

        let payload = NotificationPayload {
            message: final_message,
            level,
            kind,
            task_key: task_key.map(|k| k.to_string()),
            reply_markup,
        };

        tracing::info!(
            module = "captain-runtime-notify",
            "[notify] emitting {:?} notification: {}",
            level,
            message
        );

        self.bus.send(BusPayload::Notification(payload));
    }

    /// Flush batched LOW/NORMAL notifications as a single "Captain summary" message.
    /// Call at the end of a captain tick.
    #[tracing::instrument(skip_all)]
    pub async fn flush_batch(&self) {
        if !self.notifications_enabled {
            return;
        }

        let messages: Vec<String> = {
            let mut batch = match self.batch.lock() {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(
                        module = "captain-runtime-notify",
                        "batch mutex poisoned during flush: {e}"
                    );
                    return;
                }
            };
            std::mem::take(&mut *batch)
        };

        if messages.is_empty() {
            return;
        }

        let count = messages.len();
        let combined = if count == 1 {
            // Guarded by `count == 1` above, so `next()` is always Some.
            match messages.into_iter().next() {
                Some(msg) => msg,
                None => return,
            }
        } else {
            let mut digest = String::from("\u{1f4cb} <b>Captain summary</b>\n");
            for msg in &messages {
                digest.push_str("\n\u{2022} ");
                digest.push_str(msg);
            }
            digest
        };

        let payload = NotificationPayload {
            message: combined,
            level: NotifyLevel::Normal,
            kind: NotificationKind::Generic,
            task_key: None,
            reply_markup: None,
        };

        tracing::info!(
            module = "captain-runtime-notify",
            "[notify] flushing {} batched notifications",
            count
        );

        self.bus.send(BusPayload::Notification(payload));
    }

    /// Convenience: send a NORMAL-level notification.
    #[tracing::instrument(skip_all)]
    pub async fn normal(&self, msg: &str) {
        self.notify(msg, NotifyLevel::Normal).await;
    }

    /// Convenience: send a HIGH-level notification.
    #[tracing::instrument(skip_all)]
    pub async fn high(&self, msg: &str) {
        self.notify(msg, NotifyLevel::High).await;
    }

    /// Convenience: send a CRITICAL-level notification.
    #[tracing::instrument(skip_all)]
    pub async fn critical(&self, msg: &str) {
        self.notify(msg, NotifyLevel::Critical).await;
    }

    /// Emit a notification if the CC result contains a rate limit warning or rejection.
    ///
    /// Tier-based denoising (80 / 85 / 90 / 95 / 99 %):
    /// - Each tier notifies once per 7-day window.
    /// - `Rejected` always fires immediately.
    /// - `Allowed` clears all tiers (recovery).
    #[tracing::instrument(skip_all)]
    pub async fn check_rate_limit<T>(
        &self,
        result: &global_claude::CcResult<T>,
        pool: &sqlx::SqlitePool,
        credential_id: Option<i64>,
    ) {
        let rl = match &result.rate_limit {
            Some(rl) => rl,
            None => return,
        };

        if rl.status == global_claude::RateLimitStatus::Allowed {
            // Recovery — clear tier state so the next warning cycle starts fresh.
            match RATE_LIMIT_TRACKER.lock() {
                Ok(mut tracker) => tracker.clear(),
                Err(e) => {
                    tracing::error!(
                        module = "captain-runtime-notify",
                        "rate limit tracker mutex poisoned: {e}"
                    );
                }
            }
            if let Some(cid) = credential_id {
                super::credential_rate_limit::clear(pool, cid).await;
            } else {
                super::ambient_rate_limit::clear();
            }
            return;
        }

        // Resolve account label only for paths that emit a notification.
        let account_suffix = match credential_id {
            Some(id) => match settings::credentials::labels_by_ids(pool, &[id]).await {
                Ok(labels) => match labels.get(&id) {
                    Some(label) => {
                        let escaped = global_infra::html::escape_html(label);
                        format!(" (account: {escaped})")
                    }
                    None => format!(" (credential #{id})"),
                },
                Err(e) => {
                    tracing::warn!(
                        module = "captain-runtime-notify",
                        "failed to look up credential label for id {id}: {e}"
                    );
                    format!(" (credential #{id})")
                }
            },
            None => " (host account)".to_string(),
        };

        match &rl.status {
            global_claude::RateLimitStatus::Rejected => {
                if let Some(cid) = credential_id {
                    super::credential_rate_limit::activate(
                        pool,
                        cid,
                        rl.resets_at,
                        rl.rate_limit_type.as_deref(),
                    )
                    .await;
                } else {
                    super::ambient_rate_limit::activate(rl.resets_at);
                }
                let msg = format!(
                    "Rate limited — request rejected (resets at {}){}",
                    rl.resets_at
                        .map(|t| {
                            let secs = t as i64;
                            time::OffsetDateTime::from_unix_timestamp(secs)
                                .map(|dt| {
                                    dt.format(&time::format_description::well_known::Rfc3339)
                                        .unwrap_or_else(|_| t.to_string())
                                })
                                .unwrap_or_else(|_| t.to_string())
                        })
                        .unwrap_or_else(|| "unknown".into()),
                    account_suffix
                );
                self.emit_rate_limit(
                    &msg,
                    NotifyLevel::High,
                    api_types::CredentialRateLimitStatus::Rejected,
                    rl,
                )
                .await;
            }
            global_claude::RateLimitStatus::AllowedWarning => {
                let pct = match rl.utilization {
                    Some(u) => (u * 100.0) as u32,
                    None => {
                        // No utilization data — always notify (the API is
                        // telling us we're approaching the limit).
                        let msg =
                            format!("Rate limit warning — utilization unknown{account_suffix}");
                        self.emit_rate_limit(
                            &msg,
                            NotifyLevel::Normal,
                            api_types::CredentialRateLimitStatus::AllowedWarning,
                            rl,
                        )
                        .await;
                        return;
                    }
                };

                let should_fire = match RATE_LIMIT_TRACKER.lock() {
                    Ok(mut tracker) => tracker.should_notify_warning(pct).is_some(),
                    Err(e) => {
                        tracing::error!(
                            module = "captain-runtime-notify",
                            "rate limit tracker mutex poisoned: {e}"
                        );
                        return;
                    }
                };
                if !should_fire {
                    tracing::debug!(
                        module = "notify",
                        utilization_pct = pct,
                        "rate limit warning suppressed — tier already notified"
                    );
                    return;
                }

                let msg = format!("Rate limit warning — {}% utilization{account_suffix}", pct);
                self.emit_rate_limit(
                    &msg,
                    NotifyLevel::Normal,
                    api_types::CredentialRateLimitStatus::AllowedWarning,
                    rl,
                )
                .await;
            }
            _ => {}
        }
    }

    /// Emit a rate-limit notification with the full payload.
    async fn emit_rate_limit(
        &self,
        message: &str,
        level: NotifyLevel,
        status: api_types::CredentialRateLimitStatus,
        rl: &global_claude::RateLimitEvent,
    ) {
        self.notify_typed(
            message,
            level,
            NotificationKind::RateLimited {
                status,
                utilization: rl.utilization,
                resets_at: rl.resets_at,
                rate_limit_type: rl.rate_limit_type.clone(),
                overage_status: api_rate_limit_status(rl.overage_status.as_ref()),
                overage_resets_at: rl.overage_resets_at,
                overage_disabled_reason: rl.overage_disabled_reason.clone(),
            },
            Some("rate-limit"),
        )
        .await;
    }
}

fn api_rate_limit_status(
    status: Option<&global_claude::RateLimitStatus>,
) -> Option<api_types::CredentialRateLimitStatus> {
    match status {
        Some(global_claude::RateLimitStatus::Allowed) => {
            Some(api_types::CredentialRateLimitStatus::Allowed)
        }
        Some(global_claude::RateLimitStatus::AllowedWarning) => {
            Some(api_types::CredentialRateLimitStatus::AllowedWarning)
        }
        Some(global_claude::RateLimitStatus::Rejected) => {
            Some(api_types::CredentialRateLimitStatus::Rejected)
        }
        Some(global_claude::RateLimitStatus::Unknown(_)) | None => None,
    }
}
