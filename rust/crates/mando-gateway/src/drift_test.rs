//! PR #855 drift-catch harness — always compiled, runs only when the
//! `drift-test` Cargo feature is on **and** `DRIFT_SEED` env var is set.
//!
//! The `DriftAware<T>` wrapper is the handler-side entry point. Handlers
//! instrumented with this wrapper return `DriftAware<ResponseDto>` instead
//! of `Json<ResponseDto>` — the type name is preserved for the parity
//! script's handler-return check.
//!
//! `IntoResponse` semantics:
//! - Default build or `DRIFT_SEED` unset: serializes `value` identically
//!   to `Json<T>` (pass-through).
//! - `drift-test` feature + `DRIFT_SEED=<route_key>.<field>` matches: the
//!   response is serialized via `serde_json::Value`, the named `field` is
//!   removed from the `Value` tree, and the mutated JSON is written to the
//!   wire as the response body. A strict typed client on the other end
//!   decodes this with `missing field X`.
//!
//! The compile-time guard below prevents accidental release builds with
//! drift-test on.
//!
//! ```text
//! $ DRIFT_SEED=getHealthSystem.version \
//!     cargo run --features drift-test --bin mando-gw -- --port 18799
//! # client: mando health  → decodes `api_types::SystemHealthResponse` and
//! # emits `missing field `version`` because the wire payload has no
//! # `version` key.
//! ```

#[cfg(all(feature = "drift-test", not(debug_assertions)))]
compile_error!("drift-test feature cannot ship in release builds");

use serde::Serialize;
#[allow(unused_imports)]
use serde_json::Value;

/// Wrapper that an instrumented handler returns in place of `axum::Json<T>`.
#[allow(dead_code)]
pub struct DriftAware<T: Serialize> {
    route_key: &'static str,
    value: T,
}

impl<T: Serialize> DriftAware<T> {
    #[allow(dead_code)]
    pub fn new(route_key: &'static str, value: T) -> Self {
        Self { route_key, value }
    }
}

impl<T: Serialize> axum::response::IntoResponse for DriftAware<T> {
    fn into_response(self) -> axum::response::Response {
        // Default path: no drift-test feature, no mutation, behaves as Json<T>.
        #[cfg(not(feature = "drift-test"))]
        {
            axum::Json(self.value).into_response()
        }

        #[cfg(feature = "drift-test")]
        {
            if let Ok(seed) = std::env::var("DRIFT_SEED") {
                if let Some((route, field)) = seed.split_once('.') {
                    if route == self.route_key {
                        if let Ok(mut v) = serde_json::to_value(&self.value) {
                            drop_field_recursive(&mut v, field);
                            return axum::Json(v).into_response();
                        }
                    }
                }
            }
            axum::Json(self.value).into_response()
        }
    }
}

#[allow(dead_code)]
fn drop_field_recursive(v: &mut Value, field: &str) {
    match v {
        Value::Object(map) => {
            map.remove(field);
            for (_, child) in map.iter_mut() {
                drop_field_recursive(child, field);
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                drop_field_recursive(child, field);
            }
        }
        _ => {}
    }
}

#[cfg(all(test, feature = "drift-test"))]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct SampleStrict {
        id: i64,
        name: String,
    }

    #[test]
    fn drop_field_recursive_removes_named_field() {
        let mut v = serde_json::json!({"id": 1, "name": "a", "nested": {"name": "b"}});
        drop_field_recursive(&mut v, "name");
        assert_eq!(v["id"], 1);
        assert!(v.get("name").is_none());
        assert!(v["nested"].get("name").is_none());
    }

    #[test]
    fn strict_deserialize_rejects_dropped_field() {
        let mut v = serde_json::json!({"id": 1, "name": "a"});
        drop_field_recursive(&mut v, "name");
        let parsed: Result<SampleStrict, _> = serde_json::from_value(v);
        let err = parsed.expect_err("strict deserialize must reject missing field");
        assert!(
            err.to_string().contains("missing field `name`"),
            "error should identify the missing field, got: {err}"
        );
    }
}
