//! Feature-gated wire-contract drift verification helpers.

#[cfg(all(feature = "drift-test", not(debug_assertions)))]
compile_error!("drift-test feature cannot ship in release builds");

#[allow(unused_imports)]
use serde_json::Value;

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
