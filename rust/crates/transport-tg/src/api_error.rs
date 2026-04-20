//! Typed error ontology + response decoding for the Telegram Bot API.
//!
//! Split out of `api.rs` to keep the HTTP-client module under the file-length
//! limit. The public surface inside the crate is the same as before the split:
//! `api.rs` re-exports `TelegramApiError`, `classify_tg_error`,
//! `classify_send_error`, and `decode_api_response`.

use std::fmt;

use anyhow::Result;
use global_infra::retry::RetryVerdict;
use serde::Deserialize;
use serde_json::Value;

/// Typed Telegram API error. Drives retry classification and downstream
/// handling without string-matching on formatted error messages.
#[derive(Debug)]
pub(crate) enum TelegramApiError {
    /// Telegram returned a rate-limit response (`error_code = 429`).
    RateLimited { description: String },
    /// Telegram (or an upstream proxy) returned an HTTP 5xx response.
    ServerError { status: u16, description: String },
    /// Transport-level connection failure (refused, reset, DNS).
    Connection { source: reqwest::Error },
    /// Transport-level request timeout.
    Timeout { source: reqwest::Error },
    /// API-level or transport failure that is not retryable.
    Other { method: String, description: String },
}

impl fmt::Display for TelegramApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited { description } => {
                write!(f, "Telegram API rate limited: {description}")
            }
            Self::ServerError {
                status,
                description,
            } => {
                write!(f, "Telegram API server error {status}: {description}")
            }
            Self::Connection { source } => write!(f, "Telegram API connection error: {source}"),
            Self::Timeout { source } => write!(f, "Telegram API timeout: {source}"),
            Self::Other {
                method,
                description,
            } => {
                write!(f, "Telegram API {method} failed: {description}")
            }
        }
    }
}

impl std::error::Error for TelegramApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connection { source } | Self::Timeout { source } => Some(source),
            Self::RateLimited { .. } | Self::ServerError { .. } | Self::Other { .. } => None,
        }
    }
}

impl TelegramApiError {
    fn verdict(&self) -> RetryVerdict {
        match self {
            Self::RateLimited { .. }
            | Self::ServerError { .. }
            | Self::Connection { .. }
            | Self::Timeout { .. } => RetryVerdict::Transient,
            Self::Other { .. } => RetryVerdict::Permanent,
        }
    }
}

pub(crate) fn classify_tg_error(e: &anyhow::Error) -> RetryVerdict {
    e.chain()
        .find_map(|src| src.downcast_ref::<TelegramApiError>())
        .map_or(RetryVerdict::Permanent, TelegramApiError::verdict)
}

/// Wrap a reqwest send-side failure into a typed Telegram error so the retry
/// classifier can discriminate transient from permanent without string match.
pub(crate) fn classify_send_error(e: reqwest::Error, method: &str) -> anyhow::Error {
    let err = if e.is_timeout() {
        TelegramApiError::Timeout { source: e }
    } else if e.is_connect() {
        TelegramApiError::Connection { source: e }
    } else {
        TelegramApiError::Other {
            method: method.to_string(),
            description: e.to_string(),
        }
    };
    anyhow::Error::new(err)
}

/// Translate a raw reqwest response into either a decoded `ApiResponse` or a
/// typed transport error. 5xx bodies become `ServerError`; response-body read
/// failures (mid-stream disconnects, timeouts) become `Connection`/`Timeout`
/// so the retry loop can try again. Only genuine JSON decode failures on a
/// fully-received body become `Other` (permanent).
pub(crate) async fn decode_api_response(
    resp: reqwest::Response,
    method: &str,
) -> Result<ApiResponse> {
    let status = resp.status();
    if status.is_server_error() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::Error::new(TelegramApiError::ServerError {
            status: status.as_u16(),
            description: body,
        }));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| classify_send_error(e, method))?;
    serde_json::from_slice::<ApiResponse>(&bytes).map_err(|e| {
        anyhow::Error::new(TelegramApiError::Other {
            method: method.to_string(),
            description: format!("response parse failed: {e}"),
        })
    })
}

#[derive(Deserialize)]
pub(crate) struct ApiResponse {
    ok: bool,
    result: Option<Value>,
    error_code: Option<u16>,
    description: Option<String>,
    parameters: Option<ApiErrorParameters>,
}

#[derive(Deserialize)]
struct ApiErrorParameters {
    retry_after: Option<u64>,
}

impl ApiResponse {
    pub(crate) fn into_result(self, method: &str) -> Result<Value> {
        if self.ok {
            return Ok(self.result.unwrap_or(Value::Null));
        }
        let description = self.description.unwrap_or_default();
        // Per Telegram Bot API docs, rate limits always surface as
        // `error_code = 429` with an optional `parameters.retry_after`.
        // Classifying off the stable numeric code avoids breaking when
        // Telegram rewords the description.
        let is_rate_limited = self.error_code == Some(429)
            || self
                .parameters
                .as_ref()
                .and_then(|p| p.retry_after)
                .is_some();
        let err = if is_rate_limited {
            TelegramApiError::RateLimited { description }
        } else {
            TelegramApiError::Other {
                method: method.to_string(),
                description,
            }
        };
        Err(anyhow::Error::new(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_error_code_classifies_transient() {
        let resp: ApiResponse = serde_json::from_str(
            r#"{"ok": false, "error_code": 429, "description": "Too Many Requests: retry after 5", "parameters": {"retry_after": 5}}"#,
        )
        .unwrap();
        let err = resp.into_result("sendMessage").unwrap_err();
        assert!(matches!(classify_tg_error(&err), RetryVerdict::Transient));
    }

    #[test]
    fn rate_limit_retry_after_alone_classifies_transient() {
        let resp: ApiResponse = serde_json::from_str(
            r#"{"ok": false, "description": "Slow down", "parameters": {"retry_after": 10}}"#,
        )
        .unwrap();
        let err = resp.into_result("sendMessage").unwrap_err();
        assert!(matches!(classify_tg_error(&err), RetryVerdict::Transient));
    }

    #[test]
    fn non_rate_limit_description_containing_429_phrase_is_permanent() {
        let resp: ApiResponse = serde_json::from_str(
            r#"{"ok": false, "error_code": 400, "description": "Bad Request: Too Many Requests-like text"}"#,
        )
        .unwrap();
        let err = resp.into_result("sendMessage").unwrap_err();
        assert!(matches!(classify_tg_error(&err), RetryVerdict::Permanent));
    }

    #[test]
    fn server_error_is_transient() {
        let err = anyhow::Error::new(TelegramApiError::ServerError {
            status: 503,
            description: "Service Unavailable".into(),
        });
        assert!(matches!(classify_tg_error(&err), RetryVerdict::Transient));
    }

    #[test]
    fn bad_request_without_rate_limit_hint_is_permanent() {
        let err = anyhow::Error::new(TelegramApiError::Other {
            method: "sendMessage".into(),
            description: "Bad Request: chat not found".into(),
        });
        assert!(matches!(classify_tg_error(&err), RetryVerdict::Permanent));
    }

    #[test]
    fn classifier_survives_added_context() {
        let root = anyhow::Error::new(TelegramApiError::RateLimited {
            description: "Too Many Requests".into(),
        });
        let wrapped = root.context("during startup handshake");
        assert!(matches!(
            classify_tg_error(&wrapped),
            RetryVerdict::Transient
        ));
    }
}
