//! Typed errors and parsing helpers for task field updates (PATCH).

use std::fmt;

/// Typed error for task field updates — replaces string-based error classification.
#[derive(Debug)]
pub enum TaskUpdateError {
    NotFound(i64),
    InvalidStatus(String),
    InvalidFieldType {
        field: String,
        expected: &'static str,
    },
    InvalidBooleanValue {
        field: String,
        value: String,
    },
    UnknownField(String),
    FieldCannotBeNull(String),
    FieldCannotBePatched(String),
    TerminalStatusTransition(String),
    NotAnObject,
}

impl fmt::Display for TaskUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "task not found: {id}"),
            Self::InvalidStatus(s) => write!(f, "invalid status: {s}"),
            Self::InvalidFieldType { field, expected } => {
                write!(f, "invalid field type for {field}: expected {expected}")
            }
            Self::InvalidBooleanValue { field, value } => {
                write!(f, "invalid boolean value for {field}: {value}")
            }
            Self::UnknownField(s) => write!(f, "unknown field: {s}"),
            Self::FieldCannotBeNull(s) => write!(f, "field {s} cannot be null"),
            Self::FieldCannotBePatched(s) => {
                write!(f, "field {s} is derived and cannot be patched")
            }
            Self::TerminalStatusTransition(s) => {
                write!(f, "cannot transition from terminal status {s}")
            }
            Self::NotAnObject => write!(f, "updates body must be a JSON object"),
        }
    }
}

impl std::error::Error for TaskUpdateError {}

impl TaskUpdateError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::NotFound(_))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

// ── Field extraction helpers ─────────────────────────────────────────────────

pub(crate) fn expect_string_field<'a>(
    key: &str,
    value: &'a serde_json::Value,
) -> Result<&'a str, TaskUpdateError> {
    value
        .as_str()
        .ok_or_else(|| TaskUpdateError::InvalidFieldType {
            field: key.into(),
            expected: "string",
        })
}

pub(crate) fn expect_i64_field(
    key: &str,
    value: &serde_json::Value,
) -> Result<i64, TaskUpdateError> {
    value
        .as_i64()
        .ok_or_else(|| TaskUpdateError::InvalidFieldType {
            field: key.into(),
            expected: "integer",
        })
}

pub(crate) fn expect_boolish_field(
    key: &str,
    value: &serde_json::Value,
) -> Result<bool, TaskUpdateError> {
    if let Some(raw) = value.as_bool() {
        return Ok(raw);
    }

    let raw = value
        .as_str()
        .ok_or_else(|| TaskUpdateError::InvalidFieldType {
            field: key.into(),
            expected: "boolean or string",
        })?;

    match raw {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(TaskUpdateError::InvalidBooleanValue {
            field: key.into(),
            value: raw.into(),
        }),
    }
}
