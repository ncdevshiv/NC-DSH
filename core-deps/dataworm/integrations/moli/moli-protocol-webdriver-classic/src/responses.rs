use serde_json::{Value, json};

use crate::commands::required_timeout_value;
use crate::{ClassicError, ClassicErrorCode, ClassicTimeouts};

pub fn success_response(value: Value) -> Value {
    json!({ "value": value })
}

pub fn status_response(ready: bool, message: impl Into<String>) -> Value {
    success_response(json!({
        "ready": ready,
        "message": message.into(),
    }))
}

pub fn new_session_response(session_id: &str, capabilities: Value) -> Value {
    success_response(json!({
        "sessionId": session_id,
        "capabilities": capabilities,
    }))
}

pub fn timeouts_value(timeouts: ClassicTimeouts) -> Value {
    json!({
        "script": timeouts.script,
        "pageLoad": timeouts.page_load,
        "implicit": timeouts.implicit,
    })
}

pub fn parse_timeouts(
    params: &Value,
    current: ClassicTimeouts,
) -> Result<ClassicTimeouts, ClassicError> {
    let object = params.as_object().ok_or_else(|| {
        ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "timeouts must be an object",
        )
    })?;
    let mut timeouts = current;
    if object.contains_key("script") {
        timeouts.script = required_timeout_value(object, "script")?;
    }
    if object.contains_key("pageLoad") {
        timeouts.page_load = required_timeout_value(object, "pageLoad")?;
    }
    if object.contains_key("implicit") {
        timeouts.implicit = required_timeout_value(object, "implicit")?;
    }
    Ok(timeouts)
}

pub fn delete_session_response() -> Value {
    success_response(Value::Null)
}

pub fn error_response(code: ClassicErrorCode, message: impl Into<String>) -> Value {
    error_response_with_data(code, message, None)
}

pub fn error_response_with_data(
    code: ClassicErrorCode,
    message: impl Into<String>,
    data: Option<Value>,
) -> Value {
    let mut response = json!({
        "value": {
            "error": code.as_str(),
            "message": message.into(),
            "stacktrace": "",
        }
    });
    if let Some(data) = data {
        response["value"]["data"] = data;
    }
    response
}
