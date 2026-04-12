//! /api/workbenches/* route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error, not_found_or_internal};
use crate::AppState;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/workbenches", get(get_workbenches))
        .route("/api/workbenches/{id}", patch(patch_workbench))
        .route("/api/workbenches/{id}/layout", get(get_workbench_layout))
        .route(
            "/api/workbenches/{id}/layout",
            patch(patch_workbench_layout),
        )
}

// ── Layout I/O helpers ─────────────────────────────────────────────────

fn layout_dir() -> std::path::PathBuf {
    mando_types::data_dir().join("workbenches")
}

fn layout_path(wb_id: i64) -> std::path::PathBuf {
    layout_dir().join(format!("{wb_id}.json"))
}

pub fn read_layout(wb_id: i64) -> anyhow::Result<mando_types::WorkbenchLayout> {
    let path = layout_path(wb_id);
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(serde_json::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(mando_types::WorkbenchLayout::new())
        }
        Err(e) => Err(e.into()),
    }
}

pub fn write_layout(wb_id: i64, layout: &mando_types::WorkbenchLayout) -> anyhow::Result<()> {
    let path = layout_path(wb_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(layout)?;
    // Atomic write: write to a temp file, then rename to avoid partial writes.
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

// ── GET /api/workbenches ───────────────────────────────────────────────

pub(crate) async fn get_workbenches(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let items = mando_db::queries::workbenches::load_active(pool)
        .await
        .map_err(internal_error)?;
    Ok(Json(json!({ "workbenches": items })))
}

// ── PATCH /api/workbenches/:id ─────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct PatchWorkbenchBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

pub(crate) async fn patch_workbench(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PatchWorkbenchBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();

    mando_db::queries::workbenches::find_by_id(pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "workbench not found"))?;

    if let Some(ref title) = body.title {
        mando_db::queries::workbenches::update_title(pool, id, title)
            .await
            .map_err(internal_error)?;
    }
    if let Some(archived) = body.archived {
        if archived {
            mando_db::queries::workbenches::archive(pool, id)
                .await
                .map_err(internal_error)?;
        } else {
            mando_db::queries::workbenches::unarchive(pool, id)
                .await
                .map_err(internal_error)?;
        }
    }
    if let Some(pinned) = body.pinned {
        let affected = if pinned {
            mando_db::queries::workbenches::pin(pool, id)
                .await
                .map_err(internal_error)?
        } else {
            mando_db::queries::workbenches::unpin(pool, id)
                .await
                .map_err(internal_error)?
        };
        if !affected {
            return Err(error_response(
                StatusCode::CONFLICT,
                if pinned {
                    "workbench cannot be pinned (archived or deleted)"
                } else {
                    "workbench not found"
                },
            ));
        }
    }

    let updated = mando_db::queries::workbenches::find_by_id(pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "workbench not found after update"))?;

    state.bus.send(
        mando_types::BusEvent::Workbenches,
        Some(json!({ "action": "updated", "item": updated })),
    );

    Ok(Json(json!(updated)))
}

// ── GET /api/workbenches/:id/layout ────────────────────────────────────

pub(crate) async fn get_workbench_layout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    mando_db::queries::workbenches::find_by_id(pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "workbench not found"))?;

    let layout = tokio::task::spawn_blocking(move || read_layout(id))
        .await
        .map_err(internal_error)?
        .map_err(not_found_or_internal)?;

    Ok(Json(serde_json::to_value(layout).map_err(internal_error)?))
}

// ── PATCH /api/workbenches/:id/layout ──────────────────────────────────

pub(crate) async fn patch_workbench_layout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    mando_db::queries::workbenches::find_by_id(pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "workbench not found"))?;

    let layout = tokio::task::spawn_blocking(move || {
        let mut layout = read_layout(id)?;
        merge_layout_patch(&mut layout, &patch);
        write_layout(id, &layout)?;
        Ok::<_, anyhow::Error>(layout)
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    Ok(Json(serde_json::to_value(layout).map_err(internal_error)?))
}

fn merge_layout_patch(layout: &mut mando_types::WorkbenchLayout, patch: &Value) {
    if let Some(ap) = patch.get("activePanel").and_then(|v| v.as_str()) {
        layout.active_panel = Some(ap.to_string());
    }
    if let Some(order) = patch.get("panelOrder").and_then(|v| v.as_array()) {
        layout.panel_order = order
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(panels) = patch.get("panels").and_then(|v| v.as_object()) {
        for (key, val) in panels {
            if let Ok(panel) =
                serde_json::from_value::<mando_types::workbench_layout::PanelState>(val.clone())
            {
                layout.panels.insert(key.clone(), panel);
            }
        }
    }
}
