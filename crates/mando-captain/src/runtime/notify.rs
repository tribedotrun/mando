//! Captain notification delivery — emits `BusEvent::Notification` on the EventBus.
//!
//! Supports edit-in-place: repeated updates for the same task_key carry the
//! key in the payload so SSE consumers can edit their previous message.
//!
//! LOW/NORMAL notifications are batched during a captain tick and flushed as
//! a single "Captain summary" message at tick end. HIGH+ events send immediately.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use mando_shared::EventBus;
use mando_types::events::{NotificationKind, NotificationPayload};
use mando_types::notify::NotifyLevel;
use mando_types::BusEvent;

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

    /// Toggle whether BusEvent::Notification should be emitted at all.
    pub fn with_notifications_enabled(mut self, enabled: bool) -> Self {
        self.notifications_enabled = enabled;
        self
    }

    /// Send a notification if it meets the threshold and quiet-mode filter.
    pub async fn notify(&self, message: &str, level: NotifyLevel) {
        self.emit(message, level, NotificationKind::Generic, None, None)
            .await;
    }

    /// Send a typed notification with a semantic kind.
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
        reply_markup: Option<serde_json::Value>,
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
            Some(slug) => mando_shared::telegram_format::linkify_pr_refs(message, slug),
            None => message.to_string(),
        };

        // Batch LOW/NORMAL notifications (no task_key, no buttons) for tick-end summary.
        if level < NotifyLevel::High && task_key.is_none() && reply_markup.is_none() {
            tracing::info!("[notify] batching {:?} notification: {}", level, message);
            match self.batch.lock() {
                Ok(mut batch) => batch.push(final_message),
                Err(e) => {
                    tracing::error!("batch mutex poisoned, notification dropped: {e}");
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

        tracing::info!("[notify] emitting {:?} notification: {}", level, message);

        match serde_json::to_value(&payload) {
            Ok(val) => self.bus.send(BusEvent::Notification, Some(val)),
            Err(e) => {
                tracing::warn!(module = "notify", error = %e, "failed to serialize notification payload")
            }
        }
    }

    /// Flush batched LOW/NORMAL notifications as a single "Captain summary" message.
    /// Call at the end of a captain tick.
    pub async fn flush_batch(&self) {
        if !self.notifications_enabled {
            return;
        }

        let messages: Vec<String> = {
            let mut batch = match self.batch.lock() {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("batch mutex poisoned during flush: {e}");
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
            messages.into_iter().next().unwrap()
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

        tracing::info!("[notify] flushing {} batched notifications", count);

        match serde_json::to_value(&payload) {
            Ok(val) => self.bus.send(BusEvent::Notification, Some(val)),
            Err(e) => {
                tracing::warn!(module = "notify", error = %e, "failed to serialize batch notification payload")
            }
        }
    }

    /// Convenience: send a NORMAL-level notification.
    pub async fn normal(&self, msg: &str) {
        self.notify(msg, NotifyLevel::Normal).await;
    }

    /// Convenience: send a HIGH-level notification.
    pub async fn high(&self, msg: &str) {
        self.notify(msg, NotifyLevel::High).await;
    }

    /// Convenience: send a CRITICAL-level notification.
    pub async fn critical(&self, msg: &str) {
        self.notify(msg, NotifyLevel::Critical).await;
    }

    /// Emit a notification if the CC result contains a rate limit warning or rejection.
    ///
    /// Tier-based denoising (80 / 85 / 90 / 95 / 99 %):
    /// - Each tier notifies once per 7-day window.
    /// - `Rejected` always fires immediately.
    /// - `Allowed` clears all tiers (recovery).
    pub async fn check_rate_limit(
        &self,
        result: &mando_cc::CcResult,
        pool: &sqlx::SqlitePool,
        credential_id: Option<i64>,
    ) {
        let rl = match &result.rate_limit {
            Some(rl) => rl,
            None => return,
        };

        if rl.status == mando_cc::RateLimitStatus::Allowed {
            // Recovery — clear tier state so the next warning cycle starts fresh.
            match RATE_LIMIT_TRACKER.lock() {
                Ok(mut tracker) => tracker.clear(),
                Err(e) => {
                    tracing::error!("rate limit tracker mutex poisoned: {e}");
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
            Some(id) => match mando_db::queries::credentials::labels_by_ids(pool, &[id]).await {
                Ok(labels) => match labels.get(&id) {
                    Some(label) => {
                        let escaped = mando_shared::telegram_format::escape_html(label);
                        format!(" (account: {escaped})")
                    }
                    None => format!(" (credential #{id})"),
                },
                Err(e) => {
                    tracing::warn!("failed to look up credential label for id {id}: {e}");
                    format!(" (credential #{id})")
                }
            },
            None => " (host account)".to_string(),
        };

        match &rl.status {
            mando_cc::RateLimitStatus::Rejected => {
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
                self.emit_rate_limit(&msg, NotifyLevel::High, "rejected", rl)
                    .await;
            }
            mando_cc::RateLimitStatus::AllowedWarning => {
                let pct = match rl.utilization {
                    Some(u) => (u * 100.0) as u32,
                    None => {
                        // No utilization data — always notify (the API is
                        // telling us we're approaching the limit).
                        let msg =
                            format!("Rate limit warning — utilization unknown{account_suffix}");
                        self.emit_rate_limit(&msg, NotifyLevel::Normal, "allowed_warning", rl)
                            .await;
                        return;
                    }
                };

                let should_fire = match RATE_LIMIT_TRACKER.lock() {
                    Ok(mut tracker) => tracker.should_notify_warning(pct).is_some(),
                    Err(e) => {
                        tracing::error!("rate limit tracker mutex poisoned: {e}");
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
                self.emit_rate_limit(&msg, NotifyLevel::Normal, "allowed_warning", rl)
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
        status_str: &str,
        rl: &mando_cc::RateLimitEvent,
    ) {
        self.notify_typed(
            message,
            level,
            NotificationKind::RateLimited {
                status: status_str.to_string(),
                utilization: rl.utilization,
                resets_at: rl.resets_at,
                rate_limit_type: rl.rate_limit_type.clone(),
                overage_status: rl.overage_status.as_ref().map(|s| s.as_str().to_string()),
                overage_resets_at: rl.overage_resets_at,
                overage_disabled_reason: rl.overage_disabled_reason.clone(),
            },
            Some("rate-limit"),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifier_new_defaults() {
        let bus = Arc::new(EventBus::new());
        let n = Notifier::new(bus);
        assert_eq!(n.threshold, NotifyLevel::Low);
        assert!(!n.quiet_mode);
        assert!(n.notifications_enabled);
    }

    #[tokio::test]
    async fn high_notification_emits_immediately() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);
        n.notify("test message", NotifyLevel::High).await;

        let (event, data) = rx.recv().await.unwrap();
        assert_eq!(event, BusEvent::Notification);
        let payload: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert_eq!(payload.message, "test message");
    }

    #[tokio::test]
    async fn normal_notification_is_batched() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);
        n.notify("low-priority update", NotifyLevel::Normal).await;

        // Should NOT emit immediately — batched.
        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(
            result.is_err(),
            "should timeout — normal notifications are batched"
        );

        // Flush should emit combined message.
        n.flush_batch().await;
        let (event, data) = rx.recv().await.unwrap();
        assert_eq!(event, BusEvent::Notification);
        let payload: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(payload.message.contains("low-priority update"));
    }

    #[tokio::test]
    async fn flush_batch_combines_multiple() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);
        n.normal("msg one").await;
        n.normal("msg two").await;

        n.flush_batch().await;
        let (_, data) = rx.recv().await.unwrap();
        let payload: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(payload.message.contains("Captain summary"));
        assert!(payload.message.contains("msg one"));
        assert!(payload.message.contains("msg two"));
    }

    #[tokio::test]
    async fn flush_batch_noop_when_empty() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);
        n.flush_batch().await;

        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "no batched items — should not emit");
    }

    #[tokio::test]
    async fn notify_below_threshold_skipped() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let mut n = Notifier::new(bus);
        n.threshold = NotifyLevel::High;
        n.notify("test", NotifyLevel::Low).await;

        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "should timeout — no event emitted");
    }

    #[tokio::test]
    async fn notify_quiet_mode_suppresses_non_high() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let mut n = Notifier::new(bus);
        n.quiet_mode = true;
        n.notify("test", NotifyLevel::Normal).await;

        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "should timeout — suppressed by quiet mode");
    }

    #[tokio::test]
    async fn notifications_disabled_suppresses_all_notification_events() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus).with_notifications_enabled(false);

        n.notify("high-priority update", NotifyLevel::High).await;
        n.normal("batched update").await;
        n.flush_batch().await;

        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "should timeout — notifications disabled");
    }

    #[test]
    fn fork_preserves_delivery_settings() {
        let bus = Arc::new(EventBus::new());
        let mut parent = Notifier::new(bus).with_notifications_enabled(false);
        parent.threshold = NotifyLevel::High;
        parent.quiet_mode = true;
        parent.repo_slug = Some("acme/widgets".into());

        let child = parent.fork();
        assert_eq!(child.threshold, NotifyLevel::High);
        assert!(child.quiet_mode);
        assert!(!child.notifications_enabled);
        assert_eq!(child.repo_slug.as_deref(), Some("acme/widgets"));
    }

    // --- RateLimitTierTracker unit tests ---

    #[test]
    fn tier_tracker_first_warning_fires() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(80), Some(80));
    }

    #[test]
    fn tier_tracker_same_tier_suppressed() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(83), Some(80));
        // Same tier, non-decreasing utilization → suppressed.
        assert_eq!(tracker.should_notify_warning(84), None);
        assert_eq!(tracker.should_notify_warning(84), None);
    }

    #[test]
    fn tier_tracker_higher_tier_fires() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(82), Some(80));
        assert_eq!(tracker.should_notify_warning(87), Some(85));
        assert_eq!(tracker.should_notify_warning(93), Some(90));
        assert_eq!(tracker.should_notify_warning(96), Some(95));
        assert_eq!(tracker.should_notify_warning(99), Some(99));
    }

    #[test]
    fn tier_tracker_below_80_suppressed() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(75), None);
        assert_eq!(tracker.should_notify_warning(0), None);
    }

    #[test]
    fn tier_tracker_drop_implies_window_reset() {
        let mut tracker = RateLimitTierTracker::default();
        // Fire at 96% — marks tiers 80, 85, 90, 95.
        assert_eq!(tracker.should_notify_warning(96), Some(95));
        // Utilization within the same window only goes up — suppress.
        assert_eq!(tracker.should_notify_warning(97), None);
        // Drop to 82% means the window rolled over — implicit clear,
        // so tier 80 fires as the start of a fresh cycle.
        assert_eq!(tracker.should_notify_warning(82), Some(80));
        // Climbing again within the new window.
        assert_eq!(tracker.should_notify_warning(91), Some(90));
    }

    #[test]
    fn tier_tracker_same_value_no_reset() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(89), Some(85));
        // Same value — not a drop, still suppressed.
        assert_eq!(tracker.should_notify_warning(89), None);
    }

    #[test]
    fn tier_tracker_clear_resets_all() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(90), Some(90));
        tracker.clear();
        // Same tier fires again after clear.
        assert_eq!(tracker.should_notify_warning(90), Some(90));
    }

    #[test]
    fn tier_tracker_expired_tier_fires_again() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(85), Some(85));

        // Backdate the notified timestamp to 8 days ago.
        let eight_days_ago = time::OffsetDateTime::now_utc() - time::Duration::days(8);
        tracker.notified.insert(85, eight_days_ago);

        // Same tier fires again after expiry.
        assert_eq!(tracker.should_notify_warning(85), Some(85));
    }

    #[test]
    fn tier_tracker_non_expired_tier_still_suppressed() {
        let mut tracker = RateLimitTierTracker::default();
        assert_eq!(tracker.should_notify_warning(90), Some(90));

        // Backdate to 6 days ago (still within 7-day window).
        let six_days_ago = time::OffsetDateTime::now_utc() - time::Duration::days(6);
        tracker.notified.insert(90, six_days_ago);

        assert_eq!(tracker.should_notify_warning(90), None);
    }

    fn make_cc_result(status: mando_cc::RateLimitStatus, utilization: f64) -> mando_cc::CcResult {
        make_cc_result_opt(status, Some(utilization))
    }

    fn make_cc_result_opt(
        status: mando_cc::RateLimitStatus,
        utilization: Option<f64>,
    ) -> mando_cc::CcResult {
        mando_cc::CcResult {
            text: String::new(),
            structured: None,
            session_id: "test".into(),
            cost_usd: None,
            duration_ms: None,
            duration_api_ms: None,
            num_turns: None,
            errors: Vec::new(),
            envelope: serde_json::Value::Null,
            stream_path: std::path::PathBuf::new(),
            rate_limit: Some(mando_cc::RateLimitEvent {
                status,
                resets_at: Some(1773273600),
                rate_limit_type: Some("seven_day".into()),
                utilization,
                overage_status: None,
                overage_resets_at: None,
                overage_disabled_reason: None,
                raw: serde_json::Value::Null,
            }),
            pid: mando_types::Pid::new(0),
        }
    }

    async fn test_pool() -> sqlx::SqlitePool {
        mando_db::Db::open_in_memory().await.unwrap().pool().clone()
    }

    #[tokio::test]
    async fn check_rate_limit_first_warning_emits() {
        let pool = test_pool().await;
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        let result = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.89);
        n.check_rate_limit(&result, &pool, None).await;

        let (event, data) = rx.recv().await.unwrap();
        assert_eq!(event, BusEvent::Notification);
        let payload: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(payload.message.contains("89% utilization"));
        assert!(payload.message.contains("(host account)"));
    }

    #[tokio::test]
    async fn check_rate_limit_same_tier_suppressed() {
        let pool = test_pool().await;
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        let r1 = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.87);
        n.check_rate_limit(&r1, &pool, None).await;
        // Consume first notification.
        rx.recv().await.unwrap();

        // Second at same tier → suppressed.
        let r2 = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.89);
        n.check_rate_limit(&r2, &pool, None).await;

        let timeout = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(timeout.is_err(), "same tier should be suppressed");
    }

    #[tokio::test]
    async fn check_rate_limit_rejected_always_fires() {
        let pool = test_pool().await;
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        let r1 = make_cc_result(mando_cc::RateLimitStatus::Rejected, 1.0);
        n.check_rate_limit(&r1, &pool, None).await;
        let (_, data) = rx.recv().await.unwrap();
        let p: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(p.message.contains("rejected"));
        assert!(p.message.contains("(host account)"));

        // Second rejected also fires.
        let r2 = make_cc_result(mando_cc::RateLimitStatus::Rejected, 1.0);
        n.check_rate_limit(&r2, &pool, None).await;
        let (_, data2) = rx.recv().await.unwrap();
        let p2: NotificationPayload = serde_json::from_value(data2.unwrap()).unwrap();
        assert!(p2.message.contains("rejected"));
    }

    #[tokio::test]
    async fn check_rate_limit_allowed_clears_tiers() {
        let pool = test_pool().await;
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        // Fire tier 85.
        let r1 = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.87);
        n.check_rate_limit(&r1, &pool, None).await;
        rx.recv().await.unwrap();

        // Recovery.
        let r2 = make_cc_result(mando_cc::RateLimitStatus::Allowed, 0.50);
        n.check_rate_limit(&r2, &pool, None).await;

        // Same tier fires again after recovery.
        let r3 = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.87);
        n.check_rate_limit(&r3, &pool, None).await;
        let (_, data) = rx.recv().await.unwrap();
        let p: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(p.message.contains("87% utilization"));
    }

    #[tokio::test]
    async fn check_rate_limit_missing_utilization_always_fires() {
        let pool = test_pool().await;
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        let r1 = make_cc_result_opt(mando_cc::RateLimitStatus::AllowedWarning, None);
        n.check_rate_limit(&r1, &pool, None).await;

        let (_, data) = rx.recv().await.unwrap();
        let p: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(p.message.contains("utilization unknown"));
        assert!(p.message.contains("(host account)"));

        // Second call with no utilization also fires (no tier tracking).
        let r2 = make_cc_result_opt(mando_cc::RateLimitStatus::AllowedWarning, None);
        n.check_rate_limit(&r2, &pool, None).await;

        let (_, data2) = rx.recv().await.unwrap();
        let p2: NotificationPayload = serde_json::from_value(data2.unwrap()).unwrap();
        assert!(p2.message.contains("utilization unknown"));
    }

    #[tokio::test]
    async fn check_rate_limit_includes_credential_label() {
        let pool = test_pool().await;
        let cred_id = mando_db::queries::credentials::insert(&pool, "team-alpha", "tok_test", None)
            .await
            .unwrap();

        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();
        let n = Notifier::new(bus);

        // Warning with credential.
        let r1 = make_cc_result(mando_cc::RateLimitStatus::AllowedWarning, 0.92);
        n.check_rate_limit(&r1, &pool, Some(cred_id)).await;
        let (_, data) = rx.recv().await.unwrap();
        let p: NotificationPayload = serde_json::from_value(data.unwrap()).unwrap();
        assert!(
            p.message.contains("(account: team-alpha)"),
            "expected credential label in message, got: {}",
            p.message
        );

        // Rejected with credential.
        let r2 = make_cc_result(mando_cc::RateLimitStatus::Rejected, 1.0);
        n.check_rate_limit(&r2, &pool, Some(cred_id)).await;
        let (_, data2) = rx.recv().await.unwrap();
        let p2: NotificationPayload = serde_json::from_value(data2.unwrap()).unwrap();
        assert!(
            p2.message.contains("(account: team-alpha)"),
            "expected credential label in rejected message, got: {}",
            p2.message
        );

        // Unknown utilization with credential.
        let r3 = make_cc_result_opt(mando_cc::RateLimitStatus::AllowedWarning, None);
        n.check_rate_limit(&r3, &pool, Some(cred_id)).await;
        let (_, data3) = rx.recv().await.unwrap();
        let p3: NotificationPayload = serde_json::from_value(data3.unwrap()).unwrap();
        assert!(
            p3.message.contains("utilization unknown"),
            "expected utilization unknown in message, got: {}",
            p3.message
        );
        assert!(
            p3.message.contains("(account: team-alpha)"),
            "expected credential label in unknown-util message, got: {}",
            p3.message
        );
    }
}
