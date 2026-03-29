//! Captain notification delivery — emits `BusEvent::Notification` on the EventBus.
//!
//! Supports edit-in-place: repeated updates for the same task_key carry the
//! key in the payload so SSE consumers can edit their previous message.
//!
//! LOW/NORMAL notifications are batched during a captain tick and flushed as
//! a single "Captain summary" message at tick end. HIGH+ events send immediately.

use std::sync::{Arc, Mutex};

use mando_shared::EventBus;
use mando_types::events::{NotificationKind, NotificationPayload};
use mando_types::notify::NotifyLevel;
use mando_types::BusEvent;

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
    pub fn clone_bus(&self) -> Arc<EventBus> {
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

        let quiet = self.quiet_mode || mando_shared::quiet_mode::is_active();
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
            if let Ok(mut batch) = self.batch.lock() {
                batch.push(final_message);
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

        self.bus.send(
            BusEvent::Notification,
            Some(serde_json::to_value(&payload).unwrap_or_default()),
        );
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
                Err(_) => return,
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

        self.bus.send(
            BusEvent::Notification,
            Some(serde_json::to_value(&payload).unwrap_or_default()),
        );
    }

    /// Convenience: send a LOW-level notification.
    pub async fn low(&self, msg: &str) {
        self.notify(msg, NotifyLevel::Low).await;
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
    pub async fn check_rate_limit(&self, result: &mando_cc::CcResult) {
        let rl = match &result.rate_limit {
            Some(rl) => rl,
            None => return,
        };

        let (message, level, status_str) = match &rl.status {
            mando_cc::RateLimitStatus::Rejected => {
                let msg = format!(
                    "Rate limited — request rejected (resets at {})",
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
                        .unwrap_or_else(|| "unknown".into())
                );
                (msg, NotifyLevel::High, "rejected")
            }
            mando_cc::RateLimitStatus::AllowedWarning => {
                let pct = rl.utilization.map(|u| (u * 100.0) as u32).unwrap_or(0);
                let msg = format!("Rate limit warning — {}% utilization", pct);
                (msg, NotifyLevel::Normal, "allowed_warning")
            }
            _ => return,
        };

        self.notify_typed(
            &message,
            level,
            NotificationKind::RateLimited {
                status: status_str.to_string(),
                utilization: rl.utilization,
                resets_at: rl.resets_at,
                rate_limit_type: rl.rate_limit_type.clone(),
                overage_status: rl.overage_status.as_ref().map(|s| match s {
                    mando_cc::RateLimitStatus::Allowed => "allowed".to_string(),
                    mando_cc::RateLimitStatus::AllowedWarning => "allowed_warning".to_string(),
                    mando_cc::RateLimitStatus::Rejected => "rejected".to_string(),
                    mando_cc::RateLimitStatus::Unknown(v) => v.clone(),
                }),
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
}
