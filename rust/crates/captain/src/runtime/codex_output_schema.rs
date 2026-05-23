use serde::Serialize;
use serde_json::{json, Value};

pub(super) struct CodexOutputSchema(pub(super) Value);

pub(super) struct AppServerOutputSchema {
    schema: Value,
}

impl Serialize for AppServerOutputSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.schema.serialize(serializer)
    }
}

impl CodexOutputSchema {
    pub(super) fn into_app_server_schema(self) -> AppServerOutputSchema {
        let mut schema = self.0;
        normalize_strict_output_schema(&mut schema);
        AppServerOutputSchema { schema }
    }
}

fn normalize_strict_output_schema(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let required_names = map
                .get("required")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<std::collections::HashSet<_>>()
                })
                .unwrap_or_default();

            if let Some(properties) = map.get_mut("properties").and_then(Value::as_object_mut) {
                let mut property_names = properties.keys().cloned().collect::<Vec<_>>();
                property_names.sort();
                for name in &property_names {
                    if let Some(property) = properties.get_mut(name) {
                        if !required_names.contains(name) {
                            allow_schema_null(property);
                        }
                        normalize_strict_output_schema(property);
                    }
                }
                map.insert(
                    "required".to_string(),
                    Value::Array(property_names.into_iter().map(Value::String).collect()),
                );
                map.insert("additionalProperties".to_string(), Value::Bool(false));
            }

            for child in map.values_mut() {
                normalize_strict_output_schema(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_strict_output_schema(item);
            }
        }
        _ => {}
    }
}

fn allow_schema_null(value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };

    if let Some(enum_values) = map.get_mut("enum").and_then(Value::as_array_mut) {
        if !enum_values.iter().any(Value::is_null) {
            enum_values.push(Value::Null);
        }
    }

    match map.get_mut("type") {
        Some(Value::String(existing)) if existing != "null" => {
            let existing = std::mem::take(existing);
            map.insert(
                "type".to_string(),
                Value::Array(vec![
                    Value::String(existing),
                    Value::String("null".to_string()),
                ]),
            );
        }
        Some(Value::Array(types)) => {
            if !types.iter().any(|value| value.as_str() == Some("null")) {
                types.push(Value::String("null".to_string()));
            }
        }
        Some(_) => {}
        None => {
            let original = Value::Object(map.clone());
            map.clear();
            map.insert(
                "anyOf".to_string(),
                Value::Array(vec![original, json!({"type": "null"})]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::CodexOutputSchema;

    #[test]
    fn output_schema_is_made_strict_for_app_server() {
        let schema = serde_json::to_value(
            CodexOutputSchema(json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["ship", "nudge"]},
                    "report": {"type": "string"},
                    "items": {
                        "type": ["array", "null"],
                        "items": {
                            "type": "object",
                            "properties": {
                                "question": {"type": "string"},
                                "answer": {"type": ["string", "null"]}
                            },
                            "required": ["question"]
                        }
                    }
                },
                "required": ["action"]
            }))
            .into_app_server_schema(),
        )
        .unwrap();

        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["action", "items", "report"]));
        assert_eq!(
            schema["properties"]["report"]["type"],
            json!(["string", "null"])
        );
        assert_eq!(
            schema["properties"]["items"]["items"]["required"],
            json!(["answer", "question"])
        );
        assert_eq!(
            schema["properties"]["items"]["items"]["additionalProperties"],
            false
        );
    }
}
