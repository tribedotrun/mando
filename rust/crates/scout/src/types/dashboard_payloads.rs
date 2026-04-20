//! Typed payload structs for scout dashboard handler return values.
//!
//! These types are local to the scout crate and are not part of the public wire
//! contract (that lives in `api-types`). They exist so that dashboard handlers
//! can construct their `serde_json::Value` responses by serializing a typed
//! struct rather than building a `Value` map and mutating it with `val["x"]`.

use serde::{Deserialize, Serialize};

/// The full payload returned by `get_scout_item`.
///
/// This matches the shape consumed by `api_types::ScoutItem` (used in
/// `routes_scout::get_scout_item` via `decode_response`). Extra fields not
/// present on all items are `Option`.
#[derive(Debug, Serialize)]
pub(crate) struct ScoutItemPayload {
    pub id: i64,
    pub rev: i64,
    pub url: String,
    pub title: Option<String>,
    pub status: String,
    pub item_type: Option<String>,
    pub summary: Option<String>,
    pub has_summary: Option<bool>,
    pub relevance: Option<i64>,
    pub quality: Option<i64>,
    pub date_added: Option<String>,
    pub date_processed: Option<String>,
    pub added_by: Option<String>,
    pub source_name: Option<String>,
    pub date_published: Option<String>,
    pub error_count: Option<i64>,
    pub research_run_id: Option<i64>,
    #[serde(rename = "telegraphUrl")]
    pub telegraph_url: Option<String>,
}

/// The payload returned by `get_scout_article` and `ensure_scout_article`.
///
/// Matches `api_types::ScoutArticleResponse`.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ScoutArticlePayload {
    pub id: i64,
    pub title: Option<String>,
    pub article: Option<String>,
    #[serde(rename = "telegraphUrl")]
    pub telegraph_url: Option<String>,
}
