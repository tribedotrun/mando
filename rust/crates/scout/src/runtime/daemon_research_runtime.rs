use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures_util::FutureExt;
use serde_json::{json, Value};

use super::ScoutRuntime;

impl ScoutRuntime {
    #[tracing::instrument(skip_all)]
    pub async fn start_research(&self, topic: String, process: bool) -> Result<i64> {
        let run_id = crate::runtime::research::insert_run(&self.pool, &topic).await?;
        let runtime = self.clone();
        self.task_tracker.spawn(async move {
            let result = AssertUnwindSafe(runtime.run_research_job(run_id, &topic, process))
                .catch_unwind()
                .await;
            if let Err(panic) = result {
                let msg = panic_to_string(&panic);
                tracing::error!(module = "scout-runtime-daemon_research_runtime", run_id, panic = %msg, "research job panicked");
                if let Err(db_err) = crate::runtime::research::fail_run(&runtime.pool, run_id, &msg).await
                {
                    tracing::error!(module = "scout-runtime-daemon_research_runtime", run_id, error = %db_err, "failed to mark panicked run as failed");
                }
                runtime.bus.send(global_bus::BusPayload::Research(Some(
                    api_types::ResearchEventData {
                        action: "failed".into(),
                        run_id,
                        error: Some(msg),
                        research_prompt: None,
                        elapsed_s: None,
                        links: None,
                        errors: None,
                        added_count: None,
                    },
                )));
            }
        });
        Ok(run_id)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_research_runs(&self, limit: i64) -> Result<Vec<crate::ScoutResearchRun>> {
        crate::runtime::research::list_runs(&self.pool, limit).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_research_run_items(&self, id: i64) -> Result<Vec<crate::ScoutItem>> {
        crate::runtime::research::list_items_by_run(&self.pool, id).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_research_run(&self, id: i64) -> Result<Option<crate::ScoutResearchRun>> {
        crate::runtime::research::get_run(&self.pool, id).await
    }

    async fn run_research_job(&self, run_id: i64, topic: &str, process: bool) {
        self.bus.send(global_bus::BusPayload::Research(Some(
            api_types::ResearchEventData {
                action: "started".into(),
                run_id,
                research_prompt: Some(topic.to_string()),
                elapsed_s: None,
                links: None,
                errors: None,
                added_count: None,
                error: None,
            },
        )));

        let heartbeat_cancel = tokio_util::sync::CancellationToken::new();
        let hb_cancel = heartbeat_cancel.clone();
        let hb_bus = self.bus.clone();
        let hb_handle = tokio::spawn(async move {
            let gaps = [120u64, 120, 240, 480];
            let mut elapsed = 0u64;
            for wait in gaps {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {
                        elapsed += wait;
                        hb_bus.send(global_bus::BusPayload::Research(Some(
                            api_types::ResearchEventData {
                                action: "progress".into(),
                                run_id,
                                elapsed_s: Some(elapsed),
                                research_prompt: None,
                                links: None,
                                errors: None,
                                added_count: None,
                                error: None,
                            },
                        )));
                    }
                    _ = hb_cancel.cancelled() => return,
                }
            }
        });

        let workflow = self.settings.load_scout_workflow();
        let research_result =
            crate::runtime::research::run_research(topic, &workflow, &self.pool).await;
        heartbeat_cancel.cancel();
        global_infra::best_effort!(hb_handle.await, "scout research heartbeat join");

        match research_result {
            Ok(output) => {
                self.complete_research_run(run_id, output, process, &workflow)
                    .await
            }
            Err(err) => self.fail_research_run(run_id, &err.to_string()).await,
        }
    }

    async fn complete_research_run(
        &self,
        run_id: i64,
        output: crate::runtime::research::ResearchOutput,
        process: bool,
        workflow: &settings::ScoutWorkflow,
    ) {
        let scout_db = crate::ScoutDb::new(self.pool.clone());
        if let Err(err) = scout_db
            .record_session(
                None,
                &output.session_id,
                "scout-research",
                output.cost_usd,
                output.duration_ms,
                output.credential_id,
            )
            .await
        {
            tracing::warn!(module = "scout-runtime-daemon_research_runtime", error = %err, "failed to record research session");
        }
        self.bus.send(global_bus::BusPayload::Sessions(None));

        let max_items = workflow.agent.research_max_items;
        let mut added_count = 0i64;
        let mut errors: Vec<Value> = Vec::new();
        let mut links_json: Vec<Value> = Vec::new();

        for link in output.result.links.iter().take(max_items) {
            self.process_research_link(
                run_id,
                link,
                process,
                &mut added_count,
                &mut errors,
                &mut links_json,
            )
            .await;
        }

        if let Err(err) = crate::runtime::research::complete_run(
            &self.pool,
            run_id,
            &output.session_id,
            added_count,
        )
        .await
        {
            tracing::warn!(module = "scout-runtime-daemon_research_runtime", run_id, error = %err, "failed to complete research run");
        }

        // Fail-fast on schema drift: if link/error deserialization
        // fails we log and SKIP the "completed" event entirely rather
        // than emit a partial payload with `links: None`. The DB rows
        // already persisted so the UI recovers on its next refetch —
        // emitting no event beats misrepresenting the result set.
        let links: Vec<api_types::ResearchLink> = match serde_json::from_value(
            serde_json::Value::Array(links_json),
        ) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    module = "scout-runtime-daemon_research_runtime",
                    run_id,
                    error = %e,
                    "skipping research 'completed' event — links failed to deserialize into wire type (api-types schema drift)"
                );
                return;
            }
        };
        let errs: Vec<api_types::ResearchError> = match serde_json::from_value(
            serde_json::Value::Array(errors),
        ) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    module = "scout-runtime-daemon_research_runtime",
                    run_id,
                    error = %e,
                    "skipping research 'completed' event — errors failed to deserialize into wire type (api-types schema drift)"
                );
                return;
            }
        };
        self.bus.send(global_bus::BusPayload::Research(Some(
            api_types::ResearchEventData {
                action: "completed".into(),
                run_id,
                links: Some(links),
                added_count: Some(added_count),
                errors: Some(errs),
                research_prompt: None,
                elapsed_s: None,
                error: None,
            },
        )));
    }

    async fn process_research_link(
        &self,
        run_id: i64,
        link: &crate::runtime::research::ResearchLink,
        process: bool,
        added_count: &mut i64,
        errors: &mut Vec<Value>,
        links_json: &mut Vec<Value>,
    ) {
        match crate::add_scout_item(&self.pool, &link.url, Some(&link.title)).await {
            Ok(val) => {
                let id = Some(val.id);
                let was_added = val.added;

                if let Some(id) = id {
                    if was_added {
                        match crate::runtime::research::set_research_run_id(&self.pool, id, run_id)
                            .await
                        {
                            Ok(()) => {
                                *added_count += 1;
                            }
                            Err(err) => {
                                tracing::warn!(module = "scout-runtime-daemon_research_runtime", scout_id = id, error = %err, "failed to set research_run_id; not counting toward added_count");
                            }
                        }
                        self.send_scout_event("created", id, self.fetch_scout_payload(id).await);
                        if process {
                            self.spawn_processing(id, link.url.clone());
                        }
                    } else if process {
                        self.retry_existing_research_item(id, &link.url).await;
                    }
                }

                links_json.push(json!({
                    "url": link.url,
                    "title": link.title,
                    "type": link.link_type,
                    "reason": link.reason,
                    "id": id,
                    "added": was_added,
                }));
            }
            Err(err) => {
                errors.push(json!({
                    "url": link.url,
                    "error": err.to_string(),
                }));
            }
        }
    }

    async fn retry_existing_research_item(&self, id: i64, url: &str) {
        let current_status = match crate::get_scout_item(&self.pool, id).await {
            Ok(value) => Some(value.status.as_str().to_string()),
            Err(err) => {
                tracing::warn!(module = "scout-runtime-daemon_research_runtime", scout_id = id, error = %err, "failed to fetch scout item status");
                None
            }
        };

        match current_status.as_deref() {
            Some("error") => {
                match crate::runtime::research::reset_error_state(&self.pool, id).await {
                    Err(err) => {
                        tracing::warn!(module = "scout-runtime-daemon_research_runtime", scout_id = id, error = %err, "failed to reset error state");
                    }
                    Ok(()) => {
                        self.spawn_processing(id, url.to_string());
                        self.send_scout_event("updated", id, self.fetch_scout_payload(id).await);
                    }
                }
            }
            Some("pending") => {
                self.spawn_processing(id, url.to_string());
            }
            _ => {}
        }
    }

    async fn fail_research_run(&self, run_id: i64, error_msg: &str) {
        if let Err(db_err) = crate::runtime::research::fail_run(&self.pool, run_id, error_msg).await
        {
            tracing::warn!(module = "scout-runtime-daemon_research_runtime", run_id, error = %db_err, "failed to mark research run as failed");
        }
        self.bus.send(global_bus::BusPayload::Research(Some(
            api_types::ResearchEventData {
                action: "failed".into(),
                run_id,
                error: Some(error_msg.to_string()),
                research_prompt: None,
                elapsed_s: None,
                links: None,
                errors: None,
                added_count: None,
            },
        )));
    }

    async fn fetch_scout_payload(&self, id: i64) -> Option<api_types::ScoutItem> {
        match crate::get_scout_item(&self.pool, id).await {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::warn!(module = "scout-runtime-daemon_research_runtime", scout_id = id, error = %err, "failed to fetch scout item for SSE event");
                None
            }
        }
    }

    fn send_scout_event(&self, action: &str, id: i64, item: Option<api_types::ScoutItem>) {
        self.bus.send(global_bus::BusPayload::Scout(Some(
            api_types::ScoutEventData {
                action: Some(action.to_string()),
                item,
                id: Some(id),
            },
        )));
    }
}

pub(super) fn emit_scout_process_failed(
    bus: &global_bus::EventBus,
    id: i64,
    url: &str,
    error: &str,
) {
    let escaped_url = escape_html(url);
    let escaped_error = escape_html(error);
    let payload = api_types::NotificationPayload {
        message: format!(
            "⚠️ Scout #{id} processing failed\n<a href=\"{escaped_url}\">{escaped_url}</a>\n{escaped_error}"
        ),
        level: api_types::NotifyLevel::Normal,
        kind: api_types::NotificationKind::ScoutProcessFailed {
            scout_id: id,
            url: url.to_string(),
            error: error.to_string(),
        },
        task_key: Some(format!("scout:{id}")),
        reply_markup: None,
    };
    bus.send(global_bus::BusPayload::Notification(payload));
}

#[tracing::instrument(skip_all)]
pub(super) async fn emit_scout_processed(
    bus: &global_bus::EventBus,
    pool: &sqlx::SqlitePool,
    id: i64,
) {
    let item = match crate::get_scout_item(pool, id).await {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(module = "scout-runtime-daemon_research_runtime", scout_id = id, error = %err, "scout notification: item lookup failed");
            return;
        }
    };
    let title = item.title.unwrap_or_else(|| "Untitled".to_string());
    let relevance = item.relevance.unwrap_or(0);
    let quality = item.quality.unwrap_or(0);
    let source_name = item.source_name;
    let telegraph_url = item.telegraph_url;
    let escaped_title = escape_html(&title);
    let source_label = source_name
        .as_deref()
        .map(|source| format!(" — {}", escape_html(source)))
        .unwrap_or_default();
    let payload = api_types::NotificationPayload {
        message: format!(
            "📰 <b>{escaped_title}</b>{source_label}\nRelevance {relevance}/100 · Quality {quality}/100"
        ),
        level: api_types::NotifyLevel::Normal,
        kind: api_types::NotificationKind::ScoutProcessed {
            scout_id: id,
            title,
            relevance,
            quality,
            source_name,
            telegraph_url,
        },
        task_key: Some(format!("scout:{id}")),
        reply_markup: None,
    };
    bus.send(global_bus::BusPayload::Notification(payload));
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn panic_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        format!("panic: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic: (unknown payload)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_to_string_formats_known_payloads() {
        let str_panic: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(panic_to_string(&str_panic), "panic: boom");
        let string_panic: Box<dyn std::any::Any + Send> = Box::new(String::from("kapow"));
        assert_eq!(panic_to_string(&string_panic), "panic: kapow");
    }
}
