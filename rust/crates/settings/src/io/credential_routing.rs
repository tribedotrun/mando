//! Capacity-aware Claude routing. SQLite serializes selection and reservation.
use super::{
    credential_leases,
    credentials::{self, CredentialRow},
    usage_probe,
    usage_probe::ProbeError,
};
use anyhow::Result;
use api_types::{
    CredentialPick, CredentialRouteCandidate, CredentialRouteRequest, CredentialRouteResponse,
};
use sqlx::SqlitePool;

const FRESH_SECONDS: i64 = 300;
const RESET_GROUP_SECONDS: i64 = 3600;

fn applies_fable(model: Option<&str>) -> bool {
    // Fable is display-only for the default Opus model and all non-Fable routes.
    model.is_some_and(|m| m.to_ascii_lowercase().contains("fable"))
}

fn stale(row: &CredentialRow, now: i64, model: Option<&str>) -> bool {
    row.last_probed_at.is_none_or(|t| now - t > FRESH_SECONDS)
        || row.five_hour_reset_at.is_none_or(|t| t <= now)
        || row.seven_day_reset_at.is_none_or(|t| t <= now)
        || (applies_fable(model)
            && (row
                .fable_last_probed_at
                .is_none_or(|t| now - t > FRESH_SECONDS)
                || (row.seven_day_fable_utilization.is_some()
                    && row.seven_day_fable_reset_at.is_none_or(|t| t <= now))))
}

fn weekly_reset(row: &CredentialRow, model: Option<&str>, now: i64) -> Option<i64> {
    [
        row.seven_day_reset_at,
        if applies_fable(model) {
            row.seven_day_fable_reset_at
        } else {
            None
        },
    ]
    .into_iter()
    .flatten()
    .filter(|t| *t > now)
    .min()
}

fn unavailable(row: &CredentialRow, now: i64) -> Option<&'static str> {
    if row.disabled_at.is_some() {
        Some("profile disabled")
    } else if !row.cli_eligible {
        Some("profile excluded from CLI")
    } else if row.expires_at.is_some_and(|t| t <= now * 1000) {
        Some("credential expired")
    } else if row.rate_limit_cooldown_until.is_some_and(|t| t > now) {
        Some("profile cooling down")
    } else {
        None
    }
}

fn candidate(
    row: &CredentialRow,
    live: i64,
    pending: i64,
    request: &CredentialRouteRequest,
    now: i64,
) -> CredentialRouteCandidate {
    let mut windows = vec![
        (row.five_hour_utilization, row.five_hour_status.as_deref()),
        (row.seven_day_utilization, row.seven_day_status.as_deref()),
    ];
    if applies_fable(request.model.as_deref()) && row.seven_day_fable_utilization.is_some() {
        windows.push((
            row.seven_day_fable_utilization,
            row.seven_day_fable_status.as_deref(),
        ));
    }
    let remaining = windows.iter().try_fold(100.0_f64, |remaining, (used, _)| {
        used.filter(|v| v.is_finite())
            .map(|v| remaining.min(((1.0 - v) * 100.0).clamp(0.0, 100.0)))
    });
    let headroom = remaining.map(|v| v / (live + pending + 1) as f64);
    let reason = if request.profile.is_none() && request.current_credential_id == Some(row.id) {
        Some("current profile")
    } else if request.profile.as_ref().is_some_and(|p| p != &row.label) {
        Some("different explicit profile")
    } else if let Some(reason) = unavailable(row, now) {
        Some(reason)
    } else if stale(row, now, request.model.as_deref()) {
        Some("usage refresh required")
    } else if windows
        .iter()
        .any(|(_, status)| *status == Some("rejected"))
        || remaining == Some(0.0)
        || (row.unified_status.as_deref() == Some("rejected")
            && (applies_fable(request.model.as_deref())
                || !matches!(
                    row.representative_claim.as_deref(),
                    Some("seven_day_overage_included" | "seven_day_fable")
                )))
    {
        Some("applicable quota exhausted")
    } else if remaining.is_none() {
        Some("usage unavailable")
    } else if request.profile.is_none()
        && headroom.is_some_and(|v| v < request.min_headroom_percent.unwrap_or(5.0))
    {
        Some("insufficient headroom for another session")
    } else {
        None
    };
    CredentialRouteCandidate {
        credential_id: row.id,
        label: row.label.clone(),
        eligible: reason.is_none(),
        reason: reason.unwrap_or("available").to_string(),
        remaining_percent: remaining,
        weekly_reset_at: weekly_reset(row, request.model.as_deref(), now),
        live_sessions: live,
        pending_launches: pending,
        headroom_percent: headroom,
        last_probed_at: row.last_probed_at,
    }
}

/// Wait for an in-flight peer, but never treat its still-stale data as refreshed.
async fn refresh_one(
    pool: &SqlitePool,
    row: &CredentialRow,
    model: Option<&str>,
    now: i64,
) -> Result<()> {
    // A successful non-Fable request cannot satisfy a later explicit Fable refresh.
    let model_bucket = if applies_fable(model) {
        "fable"
    } else {
        "non_fable"
    };
    let claimed = sqlx::query("INSERT INTO claude_route_probe_attempts (credential_id, model_bucket, attempted_at, completed_at)
        VALUES (?1, ?2, ?3, NULL) ON CONFLICT(credential_id, model_bucket) DO UPDATE SET attempted_at=excluded.attempted_at, completed_at=NULL
        WHERE attempted_at <= ?4")
        .bind(row.id).bind(model_bucket).bind(now).bind(now - 60).execute(pool).await?.rows_affected() > 0;
    if !claimed {
        loop {
            let attempt: Option<(i64, Option<i64>)> = sqlx::query_as("SELECT attempted_at, completed_at FROM claude_route_probe_attempts WHERE credential_id=?1 AND model_bucket=?2")
                .bind(row.id).bind(model_bucket).fetch_optional(pool).await?;
            if attempt.is_none_or(|(started, completed)| {
                completed.is_some()
                    || time::OffsetDateTime::now_utc().unix_timestamp() - started >= 30
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        return Ok(());
    }
    let selected_model = model
        .filter(|m| *m != "default")
        .unwrap_or("claude-opus-5-5");
    match tokio::time::timeout(
        std::time::Duration::from_secs(25),
        usage_probe::probe_model(&row.access_token, selected_model),
    )
    .await
    {
        Ok(Ok(snapshot)) => {
            credentials::set_usage_snapshot(pool, row.id, &snapshot).await?;
        }
        Ok(Err(ProbeError::Unauthorized)) => {
            credentials::mark_expired(pool, row.id).await?;
        }
        Ok(Err(ProbeError::RateLimited { resets_at, claim })) => {
            let until = resets_at
                .and_then(|t| i64::try_from(t).ok())
                .unwrap_or(now + 300);
            if claim.as_deref().is_some_and(|claim| {
                claim == "seven_day_overage_included" || claim == "seven_day_fable"
            }) {
                sqlx::query("UPDATE credentials SET seven_day_fable_utilization=1.0, seven_day_fable_reset_at=?1,
                    seven_day_fable_status='rejected', fable_last_probed_at=?2 WHERE id=?3")
                    .bind(until).bind(now).bind(row.id).execute(pool).await?;
            } else {
                credentials::set_rate_limit_cooldown(pool, row.id, until).await?;
            }
        }
        Ok(Err(error)) => {
            tracing::warn!(module = "credentials", credential_id=row.id, %error, "routing quota refresh failed")
        }
        Err(error) => {
            tracing::warn!(module = "credentials", credential_id=row.id, %error, "routing quota refresh timed out")
        }
    }
    sqlx::query("UPDATE claude_route_probe_attempts SET completed_at=?1 WHERE credential_id=?2 AND model_bucket=?3")
        .bind(time::OffsetDateTime::now_utc().unix_timestamp())
        .bind(row.id)
        .bind(model_bucket)
        .execute(pool)
        .await?;
    Ok(())
}

/// Refresh only the earliest viable reset group; stop when that group has capacity.
/// The durable compare-and-set throttles concurrent requests, including failures.
async fn refresh_candidates(pool: &SqlitePool, request: &CredentialRouteRequest) -> Result<()> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut rows = credentials::list_all(pool).await?;
    rows.retain(|r| {
        r.provider == "claude"
            && unavailable(r, now).is_none()
            && request.profile.as_ref().is_none_or(|p| p == &r.label)
            && (request.profile.is_some() || request.current_credential_id != Some(r.id))
    });
    rows.sort_by_key(|r| weekly_reset(r, request.model.as_deref(), now).unwrap_or(i64::MAX));
    let counts =
        credential_leases::counts(&mut *pool.acquire().await?, now, &request.launch_id).await?;
    let mut eligible_reset: Option<i64> = None;
    for row in rows {
        if eligible_reset.is_some_and(|reset| {
            weekly_reset(&row, request.model.as_deref(), now).unwrap_or(i64::MAX)
                > reset.saturating_add(RESET_GROUP_SECONDS)
        }) {
            break;
        }
        if stale(&row, now, request.model.as_deref()) {
            refresh_one(pool, &row, request.model.as_deref(), now).await?;
        }
        if let Some(current) = credentials::get_row_by_id(pool, row.id).await? {
            let load = counts.iter().find(|c| c.credential_id == row.id);
            let c = candidate(
                &current,
                load.map_or(0, |c| c.live_sessions),
                load.map_or(0, |c| c.pending_launches),
                request,
                time::OffsetDateTime::now_utc().unix_timestamp(),
            );
            if c.eligible {
                eligible_reset =
                    Some(eligible_reset.unwrap_or(c.weekly_reset_at.unwrap_or(i64::MAX)));
            }
        }
    }
    Ok(())
}

pub(crate) async fn route(
    pool: &SqlitePool,
    request: &CredentialRouteRequest,
) -> Result<CredentialRouteResponse> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(28);
    for attempt in 0..2 {
        match tokio::time::timeout_at(deadline, refresh_candidates(pool, request)).await {
            Ok(result) => result?,
            Err(error) => {
                tracing::warn!(module = "credentials", launch_id=%request.launch_id, %error, "Claude routing refresh budget exhausted")
            }
        }
        let result = select_and_reserve(pool, request).await?;
        if result.pick.is_some() || attempt == 1 || tokio::time::Instant::now() >= deadline {
            return Ok(result);
        }
    }
    anyhow::bail!("Claude routing attempts exhausted")
}

async fn select_and_reserve(
    pool: &SqlitePool,
    request: &CredentialRouteRequest,
) -> Result<CredentialRouteResponse> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    // BEGIN IMMEDIATE acquires the writer lock before reading load, preventing two
    // concurrent routes from selecting against the same pre-reservation snapshot.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let rows: Vec<CredentialRow> =
        sqlx::query_as("SELECT * FROM credentials WHERE provider = 'claude'")
            .fetch_all(&mut *tx)
            .await?;
    let counts = credential_leases::counts(&mut tx, now, &request.launch_id).await?;
    let candidates: Vec<_> = rows
        .iter()
        .map(|row| {
            let count = counts.iter().find(|c| c.credential_id == row.id);
            candidate(
                row,
                count.map_or(0, |c| c.live_sessions),
                count.map_or(0, |c| c.pending_launches),
                request,
                now,
            )
        })
        .collect();
    let earliest = candidates
        .iter()
        .filter(|c| c.eligible)
        .filter_map(|c| c.weekly_reset_at)
        .min();
    let selected = candidates
        .iter()
        .filter(|c| c.eligible)
        .filter(|c| {
            earliest.is_none_or(|t| {
                c.weekly_reset_at
                    .is_some_and(|r| r <= t.saturating_add(RESET_GROUP_SECONDS))
            })
        })
        .max_by(|a, b| {
            a.headroom_percent
                .unwrap_or(0.0)
                .total_cmp(&b.headroom_percent.unwrap_or(0.0))
                .then_with(|| {
                    let last = |id| {
                        rows.iter()
                            .find(|r| r.id == id)
                            .and_then(|r| r.last_picked_at)
                            .unwrap_or(0)
                    };
                    last(b.credential_id).cmp(&last(a.credential_id))
                })
                .then_with(|| b.credential_id.cmp(&a.credential_id))
        });
    let (pick, reason, reservation_expires_at) = if let Some(selected) = selected {
        let row = rows
            .iter()
            .find(|r| r.id == selected.credential_id)
            .ok_or_else(|| anyhow::anyhow!("selected credential disappeared"))?;
        let reason = format!(
            "{}: {:.1}% remaining, {} live sessions, {} pending; weekly reset {}",
            row.label,
            selected.remaining_percent.unwrap_or(0.0),
            selected.live_sessions,
            selected.pending_launches,
            selected
                .weekly_reset_at
                .map_or_else(|| "unknown".into(), |t| t.to_string())
        );
        let reserve = !request.dry_run && request.current_credential_id != Some(row.id);
        if reserve {
            credential_leases::reserve(&mut tx, &request.launch_id, row.id, now).await?;
        }
        (
            Some(CredentialPick {
                id: row.id,
                label: row.label.clone(),
                token: if request.dry_run {
                    String::new()
                } else {
                    row.access_token.clone()
                },
            }),
            reason,
            reserve.then_some(now + credential_leases::LEASE_SECONDS),
        )
    } else {
        let reason = request.profile.as_ref().map_or_else(
            || "No alternative profile has fresh usable capacity".to_string(),
            |p| {
                candidates.iter().find(|c| &c.label == p).map_or_else(
                    || format!("Profile {p:?} not found"),
                    |c| format!("{}: {}", c.label, c.reason),
                )
            },
        );
        (None, reason, None)
    };
    tx.commit().await?;
    Ok(CredentialRouteResponse {
        pick,
        reason,
        candidates,
        reservation_expires_at,
    })
}

/// Read-only UI projection: no credential tokens and no probes.
pub(crate) async fn status(
    pool: &SqlitePool,
) -> Result<api_types::CredentialRoutingStatusResponse> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let rows = credentials::list_all(pool).await?;
    let counts = credential_leases::counts(&mut *pool.acquire().await?, now, "").await?;
    let request = CredentialRouteRequest {
        launch_id: String::new(),
        current_credential_id: None,
        profile: None,
        model: None,
        dry_run: true,
        min_headroom_percent: None,
    };
    Ok(api_types::CredentialRoutingStatusResponse {
        candidates: rows
            .iter()
            .filter(|r| r.provider == "claude")
            .map(|row| {
                let load = counts.iter().find(|c| c.credential_id == row.id);
                candidate(
                    row,
                    load.map_or(0, |c| c.live_sessions),
                    load.map_or(0, |c| c.pending_launches),
                    &request,
                    now,
                )
            })
            .collect(),
    })
}
