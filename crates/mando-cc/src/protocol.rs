//! Wire protocol for stream-json communication with claude CLI.

/// Build a user message for stdin in stream-json format.
pub(crate) fn user_message(content: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": content
        },
    })
}

/// Build a control response (allow) for a tool permission request.
pub(crate) fn control_response_allow(request_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": {
                "behavior": "allow"
            }
        }
    })
}

/// Build an initialize response (empty success).
pub(crate) fn control_response_init(request_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_format() {
        let msg = user_message("Hello");
        assert_eq!(msg["type"], "user");
        assert_eq!(msg["message"]["role"], "user");
        assert_eq!(msg["message"]["content"], "Hello");
    }

    #[test]
    fn allow_response_format() {
        let resp = control_response_allow("req_1");
        assert_eq!(resp["response"]["response"]["behavior"], "allow");
        assert_eq!(resp["response"]["request_id"], "req_1");
    }
}
