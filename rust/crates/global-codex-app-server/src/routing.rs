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
