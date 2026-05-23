use serde_json::Value;

pub(crate) fn notification_thread_id(value: &Value) -> Option<String> {
    value
        .pointer("/params/threadId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(crate) fn response_thread_id(method: &str, params: &Value, response: &Value) -> Option<String> {
    [
        "/result/thread/id",
        "/result/turn/threadId",
        "/result/threadId",
        "/thread/id",
        "/turn/threadId",
    ]
    .into_iter()
    .find_map(|pointer| response.pointer(pointer).and_then(Value::as_str))
    .or_else(|| {
        (method == "thread/resume")
            .then(|| params.pointer("/threadId").and_then(Value::as_str))
            .flatten()
    })
    .or_else(|| params.pointer("/threadId").and_then(Value::as_str))
    .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{notification_thread_id, response_thread_id};

    #[test]
    fn extracts_thread_id_from_turn_notifications() {
        let value = json!({"method": "turn/completed", "params": {"threadId": "t-1"}});
        assert_eq!(notification_thread_id(&value).as_deref(), Some("t-1"));
    }

    #[test]
    fn ignores_global_notifications_without_thread() {
        let value = json!({"method": "account/updated", "params": {}});
        assert_eq!(notification_thread_id(&value), None);
    }

    #[test]
    fn extracts_thread_id_from_thread_start_response() {
        let params = json!({"cwd": "/tmp/work"});
        let response = json!({"id": 101, "result": {"thread": {"id": "thread-1"}}});

        assert_eq!(
            response_thread_id("thread/start", &params, &response).as_deref(),
            Some("thread-1")
        );
    }

    #[test]
    fn extracts_thread_id_from_turn_start_params() {
        let params = json!({"threadId": "thread-2"});
        let response = json!({"id": 102, "result": {"turn": {"id": "turn-1"}}});

        assert_eq!(
            response_thread_id("turn/start", &params, &response).as_deref(),
            Some("thread-2")
        );
    }
}
