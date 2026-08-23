//! JSON-RPC 2.0 message types shared by MCP and A2A.

use serde::{Deserialize, Serialize};

/// A JSON-RPC request (with id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new(
        id: serde_json::Value,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC response (success result or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: serde_json::Value::Null,
            }),
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

/// A JSON-RPC notification (no id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_response_roundtrip() {
        let request = JsonRpcRequest::new(
            serde_json::json!(1),
            "tools/call",
            serde_json::json!({"name": "calc"}),
        );
        let wire = serde_json::to_string(&request).unwrap();
        let back: JsonRpcRequest = serde_json::from_str(&wire).unwrap();
        assert_eq!(back.method, "tools/call");
        assert_eq!(back.params["name"], "calc");

        let response = JsonRpcResponse::ok(serde_json::json!(1), serde_json::json!({"ok": true}));
        let wire = serde_json::to_string(&response).unwrap();
        let back: JsonRpcResponse = serde_json::from_str(&wire).unwrap();
        assert_eq!(back.id, 1);
        assert!(back.error.is_none());
        assert_eq!(back.result.unwrap()["ok"], true);
    }

    #[test]
    fn error_response_roundtrip() {
        let response = JsonRpcResponse::err(serde_json::json!(2), -32601, "method not found");
        let wire = serde_json::to_string(&response).unwrap();
        assert!(wire.contains("-32601"));
        let back: JsonRpcResponse = serde_json::from_str(&wire).unwrap();
        assert_eq!(back.error.unwrap().code, -32601);
    }
}
