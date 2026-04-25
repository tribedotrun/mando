use super::CaptainRuntime;

impl CaptainRuntime {
    /// Commit the `needs-clarification → clarifying` transition for a task so
    /// the subsequent `apply_clarifier_result` sees the state its queries
    /// expect. Used by the HTTP clarify route before it runs the inline
    /// re-clarification turn.
    #[tracing::instrument(skip_all)]
    pub async fn persist_resume_clarifier(&self, item: &mut crate::Task) -> anyhow::Result<()> {
        crate::service::lifecycle::apply_transition(item, crate::ItemStatus::Clarifying)?;
        // Refresh the clarifier heartbeat at the moment we enter Clarifying so
        // timeout detection starts counting from the inline call's actual start.
        item.last_activity_at = Some(global_types::now_rfc3339());
        // NOTE: we intentionally preserve `session_ids.clarifier` across the
        // NC→Clarifying transition. The previous code nulled it here so
        // `tick_clarify_poll` couldn't re-apply the prior session's already-
        // consumed stream on top of the fresh inline call (PR #887). That
        // defense is now provided by per-session `result_applied_at`
        // idempotency in `cc_sessions`: the poll skips any session whose
        // result has already been applied. And with the `dispatch_reclarify`
        // safety net removed, nothing else fires on "Clarifying + no session".
        crate::io::queries::tasks::persist_resume_clarifier(&self.pool, item).await
    }

    /// Emit a `ClarifierFailed` timeline event for a task. Used by the HTTP
    /// clarify route when `answer_and_reclarify` returns an error — the feed
    /// can then render a "CC errored — retry" card distinct from the stale
    /// "needs input" card. `session_id == ""` encodes "no CC session
    /// established" (spawn failure / pre-prompt timeout); `api_error_status
    /// == 0` encodes "non-HTTP error" per the PR #889 no-`Option` rule.
    #[tracing::instrument(skip_all)]
    pub async fn emit_clarifier_failed(
        &self,
        item: &crate::Task,
        session_id: Option<&str>,
        api_error_status: Option<u16>,
        message: &str,
    ) -> anyhow::Result<()> {
        let summary = match api_error_status {
            Some(status) => format!("Clarifier CC errored (status {status})"),
            None => "Clarifier CC errored".to_string(),
        };
        self.emit_task_timeline_event(
            item,
            &summary,
            crate::TimelineEventPayload::ClarifierFailed {
                session_id: session_id.unwrap_or("").to_string(),
                api_error_status: api_error_status.unwrap_or(0),
                message: message.to_string(),
            },
        )
        .await
    }

    /// Walk a task back from `Clarifying` to `NeedsClarification` after an
    /// inline reclarifier failure, and emit a `ClarifierFailed` timeline
    /// event carrying the error context. Leaves the in-memory `item.status`
    /// in sync with the DB.
    #[tracing::instrument(skip_all)]
    pub async fn rollback_clarifier_after_failure(
        &self,
        item: &mut crate::Task,
        session_id: Option<&str>,
        api_error_status: Option<u16>,
        message: &str,
    ) -> anyhow::Result<()> {
        crate::service::lifecycle::apply_clarifier_failure(item)?;
        let event = crate::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "http".to_string(),
            summary: match api_error_status {
                Some(status) => format!("Clarifier CC errored (status {status})"),
                None => "Clarifier CC errored".to_string(),
            },
            data: crate::TimelineEventPayload::ClarifierFailed {
                session_id: session_id.unwrap_or("").to_string(),
                api_error_status: api_error_status.unwrap_or(0),
                message: message.to_string(),
            },
        };
        crate::io::queries::tasks::persist_status_transition_with_command(
            &self.pool,
            item,
            crate::ItemStatus::Clarifying.as_str(),
            "clarifier_failed",
            &event,
        )
        .await?;
        // Drain the `task.timeline.project` outbox synchronously so the
        // `ClarifierFailed` event lands in `timeline_events` before the
        // HTTP response returns. Without this, the renderer's feed query
        // could see a stale state window.
        self.drain_pending_lifecycle_effects().await?;
        Ok(())
    }
}
