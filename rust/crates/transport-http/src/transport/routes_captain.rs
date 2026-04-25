//! /api/captain/* and /api/workers/* route handlers.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// Server-side ceiling for the drain loop iteration count. Non-wire so it
/// can be tightened without a contract bump. Callers that send a larger
/// `max_ticks` get silently clamped to this.
const DRAIN_MAX_TICKS_CEILING: u32 = 60;

/// Server-side wall-clock ceiling for the drain loop. Same reasoning as
/// `DRAIN_MAX_TICKS_CEILING` — lives in code, not wire.
const DRAIN_WALL_CLOCK_CAP: Duration = Duration::from_secs(60);

fn ui_desired_state_to_wire(state: transport_ui::UiDesiredState) -> api_types::UiDesiredState {
    match state {
        transport_ui::UiDesiredState::Running => api_types::UiDesiredState::Running,
        transport_ui::UiDesiredState::Suppressed => api_types::UiDesiredState::Suppressed,
        transport_ui::UiDesiredState::Updating => api_types::UiDesiredState::Updating,
    }
}

/// GET /api/health — lightweight liveness probe (public, no auth).
pub(crate) async fn get_health(State(state): State<AppState>) -> Json<api_types::HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();
    Json(api_types::HealthResponse {
        healthy: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        uptime,
    })
}

/// GET /api/health/system — full system info (protected, auth required).
///
/// Returns HTTP 503 if the underlying database is unreachable or the
/// captain auto-tick loop has flagged itself as degraded.
pub(crate) async fn get_health_system(
    State(state): State<AppState>,
) -> (StatusCode, Json<api_types::SystemHealthResponse>) {
    let config = state.settings.load_config();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = captain::resolve_captain_runtime_paths(&config);
    let ui_status = state.ui_runtime.status().await;
    let telegram_status = state.telegram_runtime.status().await;
    let mut healthy = true;
    let (active, total) = state
        .captain
        .health_summary_counts()
        .await
        .unwrap_or_else(|e| {
            tracing::error!(module = "transport-http-transport-routes_captain", error = %e, "failed to load captain health summary");
            healthy = false;
            (0, 0)
        });
    let captain_degraded = state.captain.health_degraded();
    if captain_degraded {
        healthy = false;
    }
    if telegram_status.enabled && !telegram_status.running {
        healthy = false;
    }
    let data_dir = global_infra::paths::data_dir();
    let uptime = state.start_time.elapsed().as_secs();
    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(api_types::SystemHealthResponse {
            healthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            pid: std::process::id(),
            uptime,
            active_workers: active,
            total_items: total,
            captain_degraded,
            projects: config
                .captain
                .projects
                .values()
                .map(|pc| pc.name.clone())
                .collect(),
            data_dir: data_dir.to_string_lossy().to_string(),
            config_path: settings::get_config_path().to_string_lossy().to_string(),
            task_db_path: active_paths.task_db_path.to_string_lossy().to_string(),
            worker_health_path: active_paths
                .worker_health_path
                .to_string_lossy()
                .to_string(),
            lockfile_path: active_paths.lockfile_path.to_string_lossy().to_string(),
            configured_task_db_path: configured_paths.task_db_path.to_string_lossy().to_string(),
            configured_worker_health_path: configured_paths
                .worker_health_path
                .to_string_lossy()
                .to_string(),
            configured_lockfile_path: configured_paths.lockfile_path.to_string_lossy().to_string(),
            restart_required: active_paths != configured_paths,
            telegram: api_types::TelegramHealth {
                enabled: telegram_status.enabled,
                running: telegram_status.running,
                owner: telegram_status.owner,
                last_error: telegram_status.last_error,
                degraded: telegram_status.degraded,
                restart_count: u64::from(telegram_status.restart_count),
                mode: telegram_status.mode,
            },
            ui: api_types::UiHealthResponse {
                desired_state: ui_desired_state_to_wire(ui_status.desired_state),
                current_pid: ui_status.current_pid,
                launch_available: ui_status.launch_available,
                running: ui_status.running,
                last_error: ui_status.last_error,
                degraded: ui_status.degraded,
                restart_count: ui_status.restart_count,
            },
        }),
    )
}

/// POST /api/captain/tick
///
/// Single-tick by default. When any of `until_idle`, `max_ticks`, or
/// `until_status` is set, the handler drains state transitions by calling
/// `trigger_captain_tick` sequentially until the requested condition is
/// met or a server-side cap trips (iteration count or wall-clock).
///
/// The response is always `TickDrainResult` — a single-tick call is just
/// drain-with-`iterations=1`, `stopped_reason=max-ticks`.
#[crate::instrument_api(method = "POST", path = "/api/captain/tick")]
pub(crate) async fn post_captain_tick(
    State(state): State<AppState>,
    Json(body): Json<api_types::TickRequest>,
) -> Result<Json<api_types::TickDrainResult>, ApiError> {
    if body.until_status.is_some() && body.task_id.is_none() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "until_status requires task_id",
        ));
    }

    let workflow = state.settings.load_captain_workflow();
    let dry_run = body.dry_run.unwrap_or(false);
    let emit_notifications = body.emit_notifications.unwrap_or(true);

    let until_idle = body.until_idle.unwrap_or(false);
    let until_status = body.until_status.clone();
    let task_id = body.task_id;
    let has_loop_signal = until_idle || until_status.is_some();
    let effective_max = clamp_max_ticks(body.max_ticks, has_loop_signal);

    let start = Instant::now();
    let mut iterations: u32 = 0;
    let mut last: Option<api_types::TickResult> = None;
    let mut prev_tasks: Option<HashMap<String, i64>> = None;

    let stopped_reason = loop {
        if let Some(reason) = should_stop_before_tick(
            iterations,
            effective_max,
            start.elapsed(),
            DRAIN_WALL_CLOCK_CAP,
        ) {
            break reason;
        }

        let tick = state
            .captain
            .trigger_captain_tick(&workflow, dry_run, emit_notifications)
            .await
            .map_err(|e| internal_error(e, "captain tick failed"))?;
        let wire = tick_result_to_wire(&tick)?;
        iterations += 1;

        let current_status = match task_id {
            Some(tid) => state
                .captain
                .task_json(tid)
                .await
                .map_err(|e| internal_error(e, "failed to load task for until_status"))?
                .map(|t| t.status),
            None => None,
        };

        let stop = should_stop_after_tick(
            until_idle,
            prev_tasks.as_ref(),
            &wire.tasks,
            until_status.as_deref(),
            current_status,
        );
        prev_tasks = Some(wire.tasks.clone());
        last = Some(wire);

        if let Some(reason) = stop {
            break reason;
        }
    };

    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(Json(api_types::TickDrainResult {
        iterations,
        stopped_reason,
        elapsed_ms,
        last: last.unwrap_or_else(empty_tick_result),
    }))
}

/// Resolve the effective iteration cap for this call.
///
/// Empty body (`{}` or only `dry_run`/`emit_notifications`) runs exactly
/// one tick. Any drain signal (`until_idle`, `until_status`) defaults to
/// the server ceiling. Explicit `max_ticks` is always clamped to the ceiling.
fn clamp_max_ticks(requested: Option<u32>, has_loop_signal: bool) -> u32 {
    match requested {
        Some(n) => n.min(DRAIN_MAX_TICKS_CEILING),
        None if has_loop_signal => DRAIN_MAX_TICKS_CEILING,
        None => 1,
    }
}

/// Cap check applied BEFORE each iteration. Returns the stop reason that
/// prevented running the next tick, or `None` to proceed.
fn should_stop_before_tick(
    iteration_count: u32,
    effective_max: u32,
    elapsed: Duration,
    wall_clock_cap: Duration,
) -> Option<api_types::DrainStop> {
    if iteration_count >= effective_max {
        return Some(api_types::DrainStop::MaxTicks);
    }
    if elapsed >= wall_clock_cap {
        return Some(api_types::DrainStop::WallClock);
    }
    None
}

/// Condition check applied AFTER each iteration. Returns the stop reason
/// the iteration satisfied, or `None` to continue draining.
fn should_stop_after_tick(
    until_idle: bool,
    prev_tasks: Option<&HashMap<String, i64>>,
    current_tasks: &HashMap<String, i64>,
    until_status: Option<&[api_types::ItemStatus]>,
    current_status: Option<api_types::ItemStatus>,
) -> Option<api_types::DrainStop> {
    if let (Some(wanted), Some(got)) = (until_status, current_status) {
        if wanted.contains(&got) {
            return Some(api_types::DrainStop::UntilStatus);
        }
    }
    if until_idle {
        if let Some(prev) = prev_tasks {
            if prev == current_tasks {
                return Some(api_types::DrainStop::Idle);
            }
        }
    }
    None
}

/// Round-trip the captain-side `TickResult` through serde into the wire
/// type. Matches the pre-drain handler, so drift stays zero between the
/// captain domain type and the `api_types` mirror.
fn tick_result_to_wire(result: &captain::TickResult) -> Result<api_types::TickResult, ApiError> {
    let val = serde_json::to_value(result)
        .map_err(|e| internal_error(e, "failed to serialize tick result"))?;
    serde_json::from_value(val).map_err(|e| internal_error(e, "failed to decode tick result"))
}

/// Placeholder used when a drain exits with zero executed iterations
/// (e.g. `max_ticks=0`). Shape matches `captain::tick_spawn::default_tick_result`.
fn empty_tick_result() -> api_types::TickResult {
    api_types::TickResult {
        mode: api_types::TickMode::Skipped,
        tick_id: None,
        max_workers: 0,
        active_workers: 0,
        tasks: HashMap::new(),
        alerts: Vec::new(),
        dry_actions: Vec::new(),
        error: None,
        rate_limited: false,
    }
}

/// POST /api/captain/triage
#[crate::instrument_api(method = "POST", path = "/api/captain/triage")]
pub(crate) async fn post_captain_triage(
    State(state): State<AppState>,
    Json(body): Json<api_types::TriageRequest>,
) -> Result<Json<api_types::TriageResponse>, ApiError> {
    match state
        .captain
        .triage_pending_review(body.item_id.as_deref())
        .await
    {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(internal_error(e, "triage failed")),
    }
}

/// POST /api/captain/stop
#[crate::instrument_api(method = "POST", path = "/api/captain/stop")]
pub(crate) async fn post_captain_stop(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::StopWorkersResponse>, ApiError> {
    match state.captain.stop_all_workers().await {
        Ok(killed) => Ok(Json(api_types::StopWorkersResponse {
            killed: killed as usize,
        })),
        Err(e) => Err(internal_error(e, "failed to stop workers")),
    }
}

/// POST /api/captain/nudge (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/captain/nudge")]
pub(crate) async fn post_captain_nudge(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::NudgeResponse>, ApiError> {
    let body = crate::image_upload_ext::extract_nudge(request).await?;
    let result = post_captain_nudge_inner(&state, &body).await;
    if result.is_err() {
        crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    }
    result
}

async fn post_captain_nudge_inner(
    state: &AppState,
    body: &crate::image_upload::NudgeWithImages,
) -> Result<Json<api_types::NudgeResponse>, ApiError> {
    let id = body.item_id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid id: {}", body.item_id),
        )
    })?;
    let workflow = state.settings.load_captain_workflow();
    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(state, &config);
    let mut item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;
    let worker_name = item
        .worker
        .clone()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "item has no worker"))?;
    let mut alerts = Vec::new();

    let message = if body.saved_images.is_empty() {
        body.message.clone()
    } else {
        format!(
            "{}{}",
            body.message,
            crate::image_upload::format_image_paths(&body.saved_images)
        )
    };

    state
        .captain
        .nudge_item(&mut item, Some(&message), &workflow, &notifier, &mut alerts)
        .await
        .map_err(|e| internal_error(e, "nudge failed"))?;

    state
        .captain
        .write_task(&item)
        .await
        .map_err(|e| internal_error(e, "failed to save task"))?;

    if !body.saved_images.is_empty() {
        if let Err(e) = state
            .captain
            .append_task_images(id, &body.saved_images)
            .await
        {
            tracing::warn!(module = "transport-http-transport-routes_captain", task_id = id, error = ?e, "failed to persist nudge images");
        }
    }

    let task_item = state.captain.task_json(id).await.ok().flatten();
    state.bus.send(global_bus::BusPayload::Tasks(Some(
        api_types::TaskEventData {
            action: Some("updated".into()),
            item: task_item.clone(),
            id: Some(id),
            cleared_by: None,
        },
    )));
    let cc_sid = task_item
        .as_ref()
        .and_then(|t| t.session_ids.as_ref())
        .and_then(|s| s.worker.as_deref())
        .unwrap_or("");
    let pid = state.captain.resolve_worker_pid(cc_sid, &worker_name);

    Ok(Json(api_types::NudgeResponse {
        ok: true,
        worker: Some(worker_name),
        pid,
        status: task_item.as_ref().map(|t| t.status),
        alerts: Some(alerts),
    }))
}

/// GET /api/workers
#[crate::instrument_api(method = "GET", path = "/api/workers")]
pub(crate) async fn get_workers(
    State(state): State<AppState>,
) -> Result<Json<api_types::WorkersResponse>, ApiError> {
    let workflow = state.settings.load_captain_workflow();
    let workers = state
        .captain
        .workers_dashboard(&workflow)
        .await
        .map_err(|e| internal_error(e, "failed to load worker dashboard"))?;
    let rl_remaining = effective_rate_limit_remaining_secs(&state)
        .await
        .map_err(|e| internal_error(e, "failed to compute rate-limit remaining secs"))?;
    Ok(Json(api_types::WorkersResponse {
        workers,
        rate_limit_remaining_secs: Some(rl_remaining),
    }))
}

/// Effective remaining cooldown seconds for the UI. Resolves to:
/// - 0 when at least one credential can spawn (or no cooldown is active),
/// - the earliest credential cooldown when credentials exist but all are cooling down,
/// - the ambient cooldown when no credentials are configured.
///
/// Propagates DB errors so a transient sqlx failure surfaces as a 500
/// rather than silently reporting "0 — ready to spawn".
async fn effective_rate_limit_remaining_secs(state: &AppState) -> anyhow::Result<u64> {
    let has_credentials = state.settings.has_any_credentials().await.unwrap_or(false);
    if !has_credentials {
        return Ok(state.captain.ambient_rate_limit_remaining_secs());
    }
    let available = state
        .settings
        .pick_worker_credential(None)
        .await
        .unwrap_or(None)
        .is_some();
    if available {
        return Ok(0);
    }
    Ok(state
        .settings
        .earliest_credential_cooldown_remaining_secs()
        .await?
        .max(0) as u64)
}

#[cfg(test)]
mod drain_loop_tests {
    use super::{
        clamp_max_ticks, should_stop_after_tick, should_stop_before_tick, DRAIN_MAX_TICKS_CEILING,
    };
    use api_types::{DrainStop, ItemStatus};
    use std::collections::HashMap;
    use std::time::Duration;

    fn counts(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    // ── clamp_max_ticks ──────────────────────────────────────────────

    #[test]
    fn no_requested_no_loop_signal_runs_one_tick() {
        assert_eq!(clamp_max_ticks(None, false), 1);
    }

    #[test]
    fn no_requested_with_loop_signal_uses_server_ceiling() {
        assert_eq!(clamp_max_ticks(None, true), DRAIN_MAX_TICKS_CEILING);
    }

    #[test]
    fn requested_under_ceiling_passes_through() {
        assert_eq!(clamp_max_ticks(Some(5), true), 5);
    }

    #[test]
    fn requested_over_ceiling_is_clamped() {
        assert_eq!(clamp_max_ticks(Some(10_000), true), DRAIN_MAX_TICKS_CEILING);
    }

    #[test]
    fn requested_zero_runs_zero_iterations() {
        assert_eq!(clamp_max_ticks(Some(0), true), 0);
    }

    // ── should_stop_before_tick ──────────────────────────────────────

    #[test]
    fn before_tick_proceeds_when_under_caps() {
        assert_eq!(
            should_stop_before_tick(0, 1, Duration::from_secs(0), Duration::from_secs(60)),
            None
        );
    }

    #[test]
    fn before_tick_stops_at_max_ticks() {
        assert_eq!(
            should_stop_before_tick(1, 1, Duration::from_secs(0), Duration::from_secs(60)),
            Some(DrainStop::MaxTicks)
        );
    }

    #[test]
    fn before_tick_stops_when_wall_clock_exceeded() {
        assert_eq!(
            should_stop_before_tick(0, 60, Duration::from_secs(61), Duration::from_secs(60)),
            Some(DrainStop::WallClock)
        );
    }

    #[test]
    fn before_tick_prefers_max_ticks_over_wall_clock() {
        // Both caps tripped — max-ticks takes precedence because the loop
        // checks iteration count first. Deterministic choice so the client
        // can read `iterations == effective_max` as the authoritative signal.
        assert_eq!(
            should_stop_before_tick(5, 5, Duration::from_secs(120), Duration::from_secs(60)),
            Some(DrainStop::MaxTicks)
        );
    }

    // ── should_stop_after_tick ───────────────────────────────────────

    #[test]
    fn after_tick_idle_on_first_iteration_is_not_idle() {
        // No `prev_tasks` means this was the first tick — can't diff yet.
        let current = counts(&[("in-progress", 1)]);
        assert_eq!(
            should_stop_after_tick(true, None, &current, None, None),
            None
        );
    }

    #[test]
    fn after_tick_idle_when_counts_unchanged() {
        let current = counts(&[("in-progress", 1), ("queued", 0)]);
        let prev = current.clone();
        assert_eq!(
            should_stop_after_tick(true, Some(&prev), &current, None, None),
            Some(DrainStop::Idle)
        );
    }

    #[test]
    fn after_tick_not_idle_when_counts_changed() {
        let prev = counts(&[("in-progress", 1)]);
        let current = counts(&[("in-progress", 0), ("merged", 1)]);
        assert_eq!(
            should_stop_after_tick(true, Some(&prev), &current, None, None),
            None
        );
    }

    #[test]
    fn after_tick_until_status_matches_any_requested() {
        let tasks = counts(&[]);
        let wanted = [ItemStatus::Merged, ItemStatus::AwaitingReview];
        assert_eq!(
            should_stop_after_tick(
                false,
                None,
                &tasks,
                Some(&wanted),
                Some(ItemStatus::AwaitingReview)
            ),
            Some(DrainStop::UntilStatus)
        );
    }

    #[test]
    fn after_tick_until_status_no_match_continues() {
        let tasks = counts(&[]);
        let wanted = [ItemStatus::Merged];
        assert_eq!(
            should_stop_after_tick(
                false,
                None,
                &tasks,
                Some(&wanted),
                Some(ItemStatus::InProgress)
            ),
            None
        );
    }

    #[test]
    fn after_tick_until_status_no_current_status_continues() {
        // Task id supplied but task missing — don't wedge on a deleted task.
        let tasks = counts(&[]);
        let wanted = [ItemStatus::Merged];
        assert_eq!(
            should_stop_after_tick(false, None, &tasks, Some(&wanted), None),
            None
        );
    }

    #[test]
    fn after_tick_until_status_beats_idle_when_both_match() {
        // Targeted-wait is more specific than idle; returning UntilStatus
        // lets callers act on the status they asked about.
        let current = counts(&[("merged", 1)]);
        let prev = current.clone();
        let wanted = [ItemStatus::Merged];
        assert_eq!(
            should_stop_after_tick(
                true,
                Some(&prev),
                &current,
                Some(&wanted),
                Some(ItemStatus::Merged)
            ),
            Some(DrainStop::UntilStatus)
        );
    }
}
