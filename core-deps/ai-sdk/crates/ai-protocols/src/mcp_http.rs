//! MCP Streamable HTTP transport (2026-07-28): client and server.
//!
//! - Requests are POSTed as JSON-RPC with the `MCP-Protocol-Version`
//!   header; responses are JSON (or SSE for subscription streams).
//! - Modern errors map to HTTP 400: missing `_meta` fields, unsupported
//!   protocol version (`-32022`), missing client capability (`-32021`),
//!   header mismatch (`-32020`).

use std::sync::Arc;

use ai_errors::{AiError, NetworkError, SerializationError};

use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::mcp::{
    META_CLIENT_CAPABILITIES, META_CLIENT_INFO, META_PROTOCOL_VERSION, McpServer,
    PROTOCOL_VERSION_2026_07_28, ServerOutcome, map_mcp_error,
};

const HEADER_PROTOCOL_VERSION: &str = "MCP-Protocol-Version";

/// The modern stateless MCP client over Streamable HTTP.
pub struct McpHttpClient {
    endpoint: String,
    http: reqwest::Client,
    protocol_version: String,
    client_info: serde_json::Value,
    client_capabilities: serde_json::Value,
    elicitation: Option<crate::mcp::ElicitationResolver>,
    sampling: Option<crate::mcp::SamplingResolver>,
    max_rounds: u32,
}

impl std::fmt::Debug for McpHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpHttpClient")
            .field("endpoint", &self.endpoint)
            .field("protocol_version", &self.protocol_version)
            .finish()
    }
}

impl McpHttpClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, AiError> {
        Ok(Self {
            endpoint: endpoint.into(),
            http: reqwest::Client::builder()
                .user_agent("ai-sdk-mcp-client/0.2")
                .build()
                .map_err(|e| NetworkError::new("mcp http client", e.to_string()))?,
            protocol_version: PROTOCOL_VERSION_2026_07_28.to_string(),
            client_info: serde_json::json!({"name": "ai-sdk-mcp-client", "version": "0.2.0"}),
            client_capabilities: serde_json::json!({"elicitation": {"form": {}}}),
            elicitation: None,
            sampling: None,
            max_rounds: 4,
        })
    }

    pub fn with_protocol_version(mut self, version: &str) -> Self {
        self.protocol_version = version.to_string();
        self
    }

    pub fn with_elicitation_resolver(mut self, resolver: crate::mcp::ElicitationResolver) -> Self {
        self.elicitation = Some(resolver);
        self
    }

    fn build_params(&self, params: serde_json::Value) -> serde_json::Value {
        let mut params = match params {
            serde_json::Value::Object(map) => map,
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".into(), other);
                map
            }
        };
        params.insert(
            "_meta".into(),
            serde_json::json!({
                META_PROTOCOL_VERSION: self.protocol_version,
                META_CLIENT_INFO: self.client_info,
                META_CLIENT_CAPABILITIES: self.client_capabilities,
            }),
        );
        serde_json::Value::Object(params)
    }

    async fn post(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, AiError> {
        let response = self
            .http
            .post(&self.endpoint)
            .header(HEADER_PROTOCOL_VERSION, &self.protocol_version)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(request)
            .send()
            .await
            .map_err(|e| NetworkError::new("mcp http post", e.to_string()))?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| NetworkError::new("mcp http body", e.to_string()))?
            .to_vec();

        if status.as_u16() == 400 {
            // Modern errors carry a JSON-RPC error body.
            if let Ok(json) = serde_json::from_slice::<JsonRpcResponse>(&body) {
                if let Some(error) = json.error {
                    return Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: json.id,
                        result: None,
                        error: Some(error),
                    });
                }
            }
        }
        if !status.is_success() {
            return Err(AiError::Network(NetworkError::new(
                "mcp http",
                format!("HTTP {status}: {}", String::from_utf8_lossy(&body)),
            )));
        }
        serde_json::from_slice(&body).map_err(|e| {
            AiError::Serialization(SerializationError::new(format!(
                "invalid MCP HTTP response: {e}"
            )))
        })
    }

    /// Calls a method with the MRTR loop (identical semantics to the
    /// duplex client).
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        let mut params = self.build_params(params);
        let mut round = 0u32;

        loop {
            let id = serde_json::json!(round + 1);
            let request = JsonRpcRequest::new(id, method, params.clone());
            let response = self.post(&request).await?;

            if let Some(error) = response.error {
                return Err(map_mcp_error(error));
            }
            let result = response.result.unwrap_or(serde_json::json!({}));

            if result.get("resultType").and_then(|r| r.as_str()) == Some("input_required") {
                round += 1;
                if round > self.max_rounds {
                    return Err(AiError::Internal(ai_errors::InternalError::new(format!(
                        "MCP request `{method}` exceeded {round} MRTR rounds"
                    ))));
                }
                let input_requests = result
                    .get("inputRequests")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let request_state = result
                    .get("requestState")
                    .and_then(|r| r.as_str())
                    .map(String::from);

                let mut input_responses = serde_json::Map::new();
                if let Some(requests) = input_requests.as_object() {
                    for (key, request) in requests {
                        let input_method =
                            request.get("method").and_then(|m| m.as_str()).unwrap_or("");
                        let input_params = request
                            .get("params")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        let response_value = match input_method {
                            "elicitation/create" => {
                                let resolver = self.elicitation.clone().ok_or_else(|| {
                                    ai_errors::InternalError::new(
                                        "server requested elicitation but no elicitation resolver is configured",
                                    )
                                })?;
                                resolver(input_params)?
                            }
                            "sampling/createMessage" => {
                                let resolver = self.sampling.clone().ok_or_else(|| {
                                    ai_errors::InternalError::new(
                                        "server requested sampling but no sampling resolver is configured",
                                    )
                                })?;
                                resolver(input_params)?
                            }
                            other => {
                                return Err(AiError::Internal(ai_errors::InternalError::new(
                                    format!("server requested unsupported input `{other}`"),
                                )));
                            }
                        };
                        input_responses.insert(key.clone(), response_value);
                    }
                }
                if let Some(object) = params.as_object_mut() {
                    object.insert(
                        "inputResponses".into(),
                        serde_json::Value::Object(input_responses),
                    );
                    if let Some(state) = &request_state {
                        object.insert("requestState".into(), serde_json::json!(state));
                    }
                }
                continue;
            }

            return Ok(result);
        }
    }

    pub async fn discover(&self) -> Result<Vec<String>, AiError> {
        let result = self.call("server/discover", serde_json::json!({})).await?;
        Ok(result
            .get("supportedVersions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn list_tools(&self) -> Result<Vec<crate::mcp::McpTool>, AiError> {
        let result = self.call("tools/list", serde_json::json!({})).await?;
        crate::mcp::parse_list(&result, "tools")
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        self.call(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        )
        .await
    }
}

/// Serves an MCP server over Streamable HTTP on a raw TCP listener.
///
/// Handles POST JSON-RPC requests; responses are JSON. Modern errors map
/// to HTTP 400 per the specification. Subscription (`subscriptions/listen`)
/// requests are answered with an SSE stream (real `text/event-stream`).
pub async fn serve_http(
    listener: tokio::net::TcpListener,
    server: Arc<McpServer>,
) -> Result<(), AiError> {
    loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|e| NetworkError::new("mcp http accept", e.to_string()))?;
        let server = server.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 65536];
            let n = match socket.read(&mut buf).await {
                Ok(n) => n,
                Err(_) => return,
            };
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = request.split("\r\n\r\n").nth(1).unwrap_or("{}").to_string();
            let protocol_version_header = request
                .lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.trim().eq_ignore_ascii_case(HEADER_PROTOCOL_VERSION))
                        .map(|(_, v)| v.trim().to_string())
                })
                .unwrap_or_default();

            let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(&body);
            let request = match parsed {
                Ok(request) => request,
                Err(e) => {
                    let response = JsonRpcResponse::err(
                        serde_json::Value::Null,
                        crate::mcp::ERROR_PARSE,
                        format!("invalid JSON: {e}"),
                    );
                    write_json_response(&mut socket, 400, &response).await;
                    return;
                }
            };

            // HTTP header must match the _meta protocol version (header
            // mismatch → -32020, HTTP 400).
            let meta_version = request
                .params
                .get("_meta")
                .and_then(|m| m.get(META_PROTOCOL_VERSION))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !protocol_version_header.is_empty()
                && !meta_version.is_empty()
                && protocol_version_header != meta_version
            {
                let response = JsonRpcResponse::err(
                    request.id.clone(),
                    crate::mcp::ERROR_HEADER_MISMATCH,
                    "MCP-Protocol-Version header does not match _meta protocolVersion",
                );
                write_json_response(&mut socket, 400, &response).await;
                return;
            }

            match server.handle_request(&request).await {
                ServerOutcome::Respond(response) => {
                    // Modern errors → HTTP 400.
                    let status = match response.error.as_ref().map(|e| e.code) {
                        Some(
                            crate::mcp::ERROR_UNSUPPORTED_PROTOCOL_VERSION
                            | crate::mcp::ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY
                            | crate::mcp::ERROR_HEADER_MISMATCH
                            | crate::mcp::ERROR_INVALID_PARAMS,
                        ) => 400,
                        _ => 200,
                    };
                    write_json_response(&mut socket, status, &response).await;
                }
                ServerOutcome::Subscription(mut handle) => {
                    // SSE stream: send notifications, then the closing
                    // result, then close.
                    let _ = socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                        )
                        .await;
                    while let Some(notification) = handle.receiver.recv().await {
                        let mut params = notification.params;
                        if let Some(obj) = params.as_object_mut() {
                            obj.insert(
                                "_meta".into(),
                                serde_json::json!({
                                    crate::mcp::META_SUBSCRIPTION_ID: handle.id.clone()
                                }),
                            );
                        }
                        let event = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": notification.method,
                            "params": params
                        });
                        let mut line = format!("data: {event}\n\n");
                        line.push_str("event: message\n\n");
                        let _ = socket.write_all(line.as_bytes()).await;
                    }
                    let result = serde_json::json!({
                        "resultType": "complete",
                        "_meta": {crate::mcp::META_SUBSCRIPTION_ID: handle.id}
                    });
                    let response = JsonRpcResponse::ok(handle.id, result);
                    let mut line = format!(
                        "data: {}\n\n",
                        serde_json::to_string(&response).unwrap_or_default()
                    );
                    line.push_str("event: message\n\n");
                    let _ = socket.write_all(line.as_bytes()).await;
                }
            }
        });
    }
}

async fn write_json_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    response: &JsonRpcResponse,
) {
    use tokio::io::AsyncWriteExt;
    let body = serde_json::to_string(response).unwrap_or_else(|_| "{}".into());
    let reason = if status == 400 { "Bad Request" } else { "OK" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = socket.write_all(head.as_bytes()).await;
    let _ = socket.write_all(body.as_bytes()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let server = Arc::new(crate::mcp::McpServer::new_with_http_test_tools());
        let handle = tokio::spawn(async move {
            let _ = serve_http(listener, server).await;
        });
        (format!("http://{addr}/mcp"), handle)
    }

    #[tokio::test]
    async fn http_roundtrip_discover_tools_call() {
        let (endpoint, server_task) = http_server();
        let client = McpHttpClient::new(endpoint).unwrap();

        let versions = client.discover().await.unwrap();
        assert!(versions.contains(&PROTOCOL_VERSION_2026_07_28.to_string()));

        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let result = client
            .call_tool("echo", serde_json::json!({"text": "over http"}))
            .await
            .unwrap();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["structuredContent"]["echo"], "over http");

        server_task.abort();
    }

    #[tokio::test]
    async fn http_unsupported_version_returns_400_with_error_body() {
        let (endpoint, server_task) = http_server();
        let client = McpHttpClient::new(endpoint)
            .unwrap()
            .with_protocol_version("1900-01-01");
        let err = client.list_tools().await.unwrap_err();
        assert!(
            err.to_string().contains("Unsupported protocol version"),
            "{err}"
        );
        let supported = crate::mcp::unsupported_versions(&err).unwrap_or_default();
        assert!(supported.contains(&PROTOCOL_VERSION_2026_07_28.to_string()));
        server_task.abort();
    }
}
