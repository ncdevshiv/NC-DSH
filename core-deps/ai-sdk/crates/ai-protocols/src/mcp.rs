//! MCP (Model Context Protocol) — modern stateless implementation
//! conforming to the **2026-07-28** revision.
//!
//! Architecture (per the 2026-07-28 specification):
//!
//! - **Stateless, per-request protocol**: there is no `initialize`
//!   handshake. Every request carries `_meta` with the required
//!   `io.modelcontextprotocol/protocolVersion` and
//!   `io.modelcontextprotocol/clientCapabilities`; servers reject requests
//!   lacking them with `-32602` and reject unsupported versions with
//!   `-32022 UnsupportedProtocolVersionError` (listing `data.supported`).
//! - **`server/discover`** (REQUIRED server method): returns
//!   `{ supportedVersions, capabilities, instructions? }`.
//! - **`resultType`** on every result (`"complete"` | `"input_required"`).
//! - **Multi Round-Trip Requests (MRTR)**: servers may answer with
//!   `InputRequiredResult { inputRequests, requestState }`; clients fulfill
//!   the input requests (e.g. `elicitation/create`) and retry the original
//!   request with `inputResponses` + `requestState`.
//! - **Elicitation** (client capability, form/url modes).
//! - **Subscriptions**: `subscriptions/listen` replaces the old HTTP GET
//!   endpoint; notifications carry `io.modelcontextprotocol/subscriptionId`.
//! - **Transports**: line-delimited stdio (duplex pipes) and Streamable
//!   HTTP (`MCP-Protocol-Version` header, JSON responses, SSE for listen
//!   streams).
//!
//! **Dual-era support**: a server may also speak the legacy
//! initialize-handshake era (2025-11-25 and earlier) for clients that do
//! not send modern per-request `_meta` (see [`McpServer::enable_legacy`]).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use ai_errors::{AiError, SerializationError};

use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// The protocol version this implementation speaks.
pub const PROTOCOL_VERSION_2026_07_28: &str = "2026-07-28";

/// Reserved `_meta` keys (spec: General fields).
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
pub const META_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";
pub const META_LOG_LEVEL: &str = "io.modelcontextprotocol/logLevel";

/// MCP-defined JSON-RPC error codes (spec: Error Codes).
pub const ERROR_PARSE: i64 = -32700;
pub const ERROR_INVALID_REQUEST: i64 = -32600;
pub const ERROR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERROR_INVALID_PARAMS: i64 = -32602;
pub const ERROR_INTERNAL: i64 = -32603;
pub const ERROR_HEADER_MISMATCH: i64 = -32020;
pub const ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY: i64 = -32021;
pub const ERROR_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// A tool exposed by an MCP server (2026-07-28 `Tool`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema (defaults to 2020-12 when `$schema` is absent).
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

impl McpTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            input_schema,
            output_schema: None,
            annotations: None,
        }
    }
}

/// Tool hints (`ToolAnnotations`) — advisory only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// A resource served by an MCP server (2026-07-28 `Resource`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// A prompt template offered by an MCP server (2026-07-28 `Prompt`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<PromptArgument>,
}

/// A prompt argument definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Client input responses + request state carried on an MRTR retry.
#[derive(Debug, Clone, Default)]
pub struct ClientInputs {
    pub input_responses: serde_json::Value,
    pub request_state: Option<String>,
}

/// The outcome of a tool/prompt handler: complete or needs more input
/// (MRTR).
#[derive(Debug, Clone)]
pub enum HandlerOutcome {
    Complete(serde_json::Value),
    /// Return an `InputRequiredResult` with the given input requests and
    /// optional opaque request state.
    NeedsInput {
        input_requests: serde_json::Value,
        request_state: Option<String>,
    },
}

/// A tool handler for the server. Handlers decide between completing and
/// requesting more input (MRTR).
#[async_trait]
pub trait McpToolHandler: Send + Sync {
    async fn call(
        &self,
        arguments: serde_json::Value,
        input: Option<ClientInputs>,
    ) -> Result<HandlerOutcome, AiError>;
}

/// A closure-based tool handler (complete-only; MRTR via
/// [`McpToolHandler`] implementations).
pub struct FunctionToolHandler {
    handler: Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value, AiError> + Send + Sync>,
}

impl FunctionToolHandler {
    pub fn new(
        handler: impl Fn(serde_json::Value) -> Result<serde_json::Value, AiError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl McpToolHandler for FunctionToolHandler {
    async fn call(
        &self,
        arguments: serde_json::Value,
        _input: Option<ClientInputs>,
    ) -> Result<HandlerOutcome, AiError> {
        (self.handler)(arguments).map(HandlerOutcome::Complete)
    }
}

/// Builds an `elicitation/create` input request (form mode, 2026-07-28).
pub fn elicitation_form_input_request(
    key: &str,
    message: &str,
    requested_schema: serde_json::Value,
) -> (String, serde_json::Value) {
    (
        key.to_string(),
        serde_json::json!({
            "method": "elicitation/create",
            "params": {
                "mode": "form",
                "message": message,
                "requestedSchema": requested_schema
            }
        }),
    )
}

/// An open subscription stream handle.
pub struct SubscriptionHandle {
    pub id: serde_json::Value,
    pub receiver: mpsc::UnboundedReceiver<JsonRpcNotification>,
    /// Notification types the client opted into.
    pub tools_list_changed: bool,
    pub prompts_list_changed: bool,
    pub resources_list_changed: bool,
    pub resource_subscriptions: Vec<String>,
}

impl std::fmt::Debug for SubscriptionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionHandle")
            .field("id", &self.id)
            .finish()
    }
}

/// What a transport should do with a request.
#[derive(Debug)]
pub enum ServerOutcome {
    /// Send this JSON-RPC response back.
    Respond(JsonRpcResponse),
    /// The request opened a long-lived subscription stream.
    Subscription(SubscriptionHandle),
}

/// The legacy protocol version served by dual-era mode.
pub const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

/// An MCP server (2026-07-28 stateless per-request; optionally dual-era).
pub struct McpServer {
    supported_versions: Vec<String>,
    legacy_enabled: bool,
    legacy_versions: Vec<String>,
    legacy_mode: std::sync::atomic::AtomicBool,
    server_name: String,
    server_version: String,
    instructions: Option<String>,
    tools: HashMap<String, (McpTool, Arc<dyn McpToolHandler>)>,
    resources: HashMap<String, (McpResource, String)>,
    prompts: HashMap<String, McpPrompt>,
    subscription_senders: std::sync::Mutex<
        Vec<(
            serde_json::Value,
            mpsc::UnboundedSender<JsonRpcNotification>,
        )>,
    >,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            supported_versions: vec![PROTOCOL_VERSION_2026_07_28.to_string()],
            legacy_enabled: false,
            legacy_versions: vec![LEGACY_PROTOCOL_VERSION.to_string()],
            legacy_mode: std::sync::atomic::AtomicBool::new(false),
            server_name: "ai-sdk-mcp".to_string(),
            server_version: "0.2.0".to_string(),
            instructions: None,
            tools: HashMap::new(),
            resources: HashMap::new(),
            prompts: HashMap::new(),
            subscription_senders: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn with_supported_versions(mut self, versions: &[&str]) -> Self {
        self.supported_versions = versions.iter().map(|v| v.to_string()).collect();
        self
    }

    /// Enables dual-era mode: clients that do not send modern per-request
    /// `_meta` are served the legacy initialize-handshake protocol
    /// (`2025-11-25` and earlier semantics). Modern requests are always
    /// served the modern way.
    pub fn enable_legacy(mut self, versions: &[&str]) -> Self {
        self.legacy_enabled = true;
        self.legacy_versions = versions.iter().map(|v| v.to_string()).collect();
        self
    }

    /// Whether the server is currently serving a legacy client.
    pub fn in_legacy_mode(&self) -> bool {
        self.legacy_mode.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn supported_versions(&self) -> &[String] {
        &self.supported_versions
    }

    pub fn register_tool(
        &mut self,
        tool: McpTool,
        handler: Arc<dyn McpToolHandler>,
    ) -> Result<(), AiError> {
        if tool.name.is_empty() {
            return Err(AiError::Tool(ai_errors::ToolError::new(
                "mcp",
                "tool name must not be empty",
            )));
        }
        self.tools.insert(tool.name.clone(), (tool, handler));
        self.notify("notifications/tools/list_changed", serde_json::json!({}));
        Ok(())
    }

    pub fn register_resource(&mut self, resource: McpResource, content: impl Into<String>) {
        self.resources
            .insert(resource.uri.clone(), (resource, content.into()));
        self.notify(
            "notifications/resources/list_changed",
            serde_json::json!({}),
        );
    }

    pub fn register_prompt(&mut self, prompt: McpPrompt) {
        self.prompts.insert(prompt.name.clone(), prompt);
        self.notify("notifications/prompts/list_changed", serde_json::json!({}));
    }

    /// Broadcasts a notification to all open subscription streams.
    pub fn notify(&self, method: &str, params: serde_json::Value) {
        let senders = self.subscription_senders.lock().unwrap();
        for (_, sender) in senders.iter() {
            let _ = sender.send(JsonRpcNotification::new(method, params.clone()));
        }
    }

    fn server_capabilities(&self) -> serde_json::Value {
        let mut capabilities = serde_json::Map::new();
        if !self.tools.is_empty() {
            capabilities.insert("tools".into(), serde_json::json!({}));
        }
        if !self.resources.is_empty() {
            capabilities.insert("resources".into(), serde_json::json!({}));
        }
        if !self.prompts.is_empty() {
            capabilities.insert("prompts".into(), serde_json::json!({}));
        }
        capabilities.insert("logging".into(), serde_json::json!({}));
        serde_json::Value::Object(capabilities)
    }

    fn server_info(&self) -> serde_json::Value {
        serde_json::json!({"name": self.server_name, "version": self.server_version})
    }

    /// Validates the per-request `_meta` fields (stateless protocol).
    /// Returns the client capabilities, or the error response to send.
    #[allow(clippy::result_large_err)] // JSON-RPC error carries full context
    fn validate_meta(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, JsonRpcResponse> {
        let id = serde_json::Value::Null; // replaced by caller
        let _ = id;
        let meta = params
            .get("_meta")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let requested_version = meta
            .get(META_PROTOCOL_VERSION)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                JsonRpcResponse::err(
                    serde_json::Value::Null,
                    ERROR_INVALID_PARAMS,
                    format!("request is missing required `_meta.{META_PROTOCOL_VERSION}`"),
                )
            })?;

        if !self
            .supported_versions
            .iter()
            .any(|v| v == requested_version)
        {
            return Err(JsonRpcResponse::err(
                serde_json::Value::Null,
                ERROR_UNSUPPORTED_PROTOCOL_VERSION,
                "Unsupported protocol version",
            )
            .with_error_data(serde_json::json!({
                "supported": self.supported_versions,
                "requested": requested_version
            })));
        }

        let capabilities = meta.get(META_CLIENT_CAPABILITIES).cloned().ok_or_else(|| {
            JsonRpcResponse::err(
                serde_json::Value::Null,
                ERROR_INVALID_PARAMS,
                format!("request is missing required `_meta.{META_CLIENT_CAPABILITIES}`"),
            )
        })?;

        Ok(capabilities)
    }

    fn result_with_meta(&self, result: serde_json::Value) -> serde_json::Value {
        let mut result = match result {
            serde_json::Value::Object(map) => map,
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".into(), other);
                map
            }
        };
        result.insert(
            "_meta".into(),
            serde_json::json!({META_SERVER_INFO: self.server_info()}),
        );
        serde_json::Value::Object(result)
    }

    /// Handles one request. Returns a response or a subscription stream.
    pub async fn handle_request(&self, request: &JsonRpcRequest) -> ServerOutcome {
        let id = request.id.clone();

        // Era detection (dual-era, spec: versioning/compatibility): a
        // request carrying modern per-request `_meta` is served statelessly;
        // an `initialize` request selects legacy semantics; anything else
        // without `_meta` on a legacy-enabled server is served legacy.
        let has_modern_meta = request
            .params
            .get("_meta")
            .and_then(|m| m.get(META_PROTOCOL_VERSION))
            .is_some();

        if !has_modern_meta {
            if self.legacy_enabled && request.method == "initialize" {
                self.legacy_mode
                    .store(true, std::sync::atomic::Ordering::Release);
                return ServerOutcome::Respond(self.legacy_initialize_response(&id));
            }
            if self.legacy_enabled && self.in_legacy_mode() {
                return self.handle_legacy_request(request);
            }
        } else {
            // A modern client resets any legacy session.
            self.legacy_mode
                .store(false, std::sync::atomic::Ordering::Release);
        }

        // Per-request _meta validation (stateless protocol).
        let capabilities = match self.validate_meta(&request.params) {
            Ok(caps) => caps,
            Err(mut error) => {
                error.id = id;
                return ServerOutcome::Respond(error);
            }
        };

        // Route.
        match request.method.as_str() {
            "server/discover" => {
                let result = self.result_with_meta(serde_json::json!({
                    "resultType": "complete",
                    "supportedVersions": self.supported_versions,
                    "capabilities": self.server_capabilities(),
                    "instructions": self.instructions,
                }));
                ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
            }
            "tools/list" => {
                let tools: Vec<&McpTool> = self.tools.values().map(|(t, _)| t).collect();
                let (page, next_cursor) = self.paginate(&request.params, &tools, 100);
                let mut result = serde_json::json!({
                    "resultType": "complete",
                    "tools": page,
                });
                if let Some(cursor) = next_cursor {
                    result["nextCursor"] = serde_json::json!(cursor);
                }
                let result = self.result_with_meta(result);
                ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
            }
            "tools/call" => {
                let outcome = self.handle_tool_call(&request.params, &capabilities).await;
                match outcome {
                    Ok(result) => ServerOutcome::Respond(JsonRpcResponse::ok(
                        id,
                        self.result_with_meta(result),
                    )),
                    Err(error) => ServerOutcome::Respond(error.with_id(id)),
                }
            }
            "resources/list" => {
                let resources: Vec<&McpResource> =
                    self.resources.values().map(|(r, _)| r).collect();
                let (page, next_cursor) = self.paginate(&request.params, &resources, 100);
                let mut result = serde_json::json!({
                    "resultType": "complete",
                    "resources": page,
                });
                if let Some(cursor) = next_cursor {
                    result["nextCursor"] = serde_json::json!(cursor);
                }
                let result = self.result_with_meta(result);
                ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
            }
            "resources/read" => {
                let uri = request
                    .params
                    .get("uri")
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                match self.resources.get(uri) {
                    Some((resource, contents)) => {
                        let result = self.result_with_meta(serde_json::json!({
                            "resultType": "complete",
                            "contents": [{
                                "uri": uri,
                                "mimeType": resource.mime_type.clone().unwrap_or_else(|| "text/plain".into()),
                                "text": contents
                            }]
                        }));
                        ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
                    }
                    None => ServerOutcome::Respond(JsonRpcResponse::err(
                        id,
                        ERROR_INVALID_PARAMS,
                        format!("unknown resource: {uri}"),
                    )),
                }
            }
            "prompts/list" => {
                let prompts: Vec<&McpPrompt> = self.prompts.values().collect();
                let (page, next_cursor) = self.paginate(&request.params, &prompts, 100);
                let mut result = serde_json::json!({
                    "resultType": "complete",
                    "prompts": page,
                });
                if let Some(cursor) = next_cursor {
                    result["nextCursor"] = serde_json::json!(cursor);
                }
                let result = self.result_with_meta(result);
                ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
            }
            "prompts/get" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                match self.prompts.get(name) {
                    Some(prompt) => {
                        let result = self.result_with_meta(serde_json::json!({
                            "resultType": "complete",
                            "description": prompt.description,
                            "messages": [{
                                "role": "user",
                                "content": {"type": "text", "text": prompt.description.clone().unwrap_or_default()}
                            }]
                        }));
                        ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
                    }
                    None => ServerOutcome::Respond(JsonRpcResponse::err(
                        id,
                        ERROR_INVALID_PARAMS,
                        format!("unknown prompt: {name}"),
                    )),
                }
            }
            "subscriptions/listen" => {
                let filter = request
                    .params
                    .get("notifications")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let (sender, receiver) = mpsc::unbounded_channel();
                self.subscription_senders
                    .lock()
                    .unwrap()
                    .push((id.clone(), sender));
                ServerOutcome::Subscription(SubscriptionHandle {
                    id,
                    receiver,
                    tools_list_changed: filter
                        .get("toolsListChanged")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    prompts_list_changed: filter
                        .get("promptsListChanged")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    resources_list_changed: filter
                        .get("resourcesListChanged")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    resource_subscriptions: filter
                        .get("resourceSubscriptions")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            }
            _ => ServerOutcome::Respond(JsonRpcResponse::err(
                id,
                ERROR_METHOD_NOT_FOUND,
                format!("method not found: {}", request.method),
            )),
        }
    }

    /// Applies cursor pagination to a list: parses the opaque `cursor`
    /// (an offset, matching the PaginatedRequestParams convention), takes
    /// at most `page_size` items, and returns the next cursor when more
    /// items remain.
    fn paginate<T>(
        &self,
        params: &serde_json::Value,
        items: &[T],
        page_size: usize,
    ) -> (Vec<T>, Option<String>)
    where
        T: Clone,
    {
        let offset = params
            .get("cursor")
            .and_then(|c| c.as_str())
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0);
        let end = (offset + page_size).min(items.len());
        let page = items[offset..end].to_vec();
        let next_cursor = if end < items.len() {
            Some(end.to_string())
        } else {
            None
        };
        (page, next_cursor)
    }

    /// The legacy `initialize` handshake response (2025-11-25 shape).
    fn legacy_initialize_response(&self, id: &serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::ok(
            id.clone(),
            serde_json::json!({
                "protocolVersion": self.legacy_versions.first().cloned().unwrap_or_else(|| LEGACY_PROTOCOL_VERSION.to_string()),
                "capabilities": self.server_capabilities(),
                "serverInfo": self.server_info(),
            }),
        )
    }

    /// Serves requests for a legacy (initialize-handshake) client: no
    /// `resultType`, no `_meta` requirements, legacy result shapes.
    fn handle_legacy_request(&self, request: &JsonRpcRequest) -> ServerOutcome {
        let id = request.id.clone();
        let result = match request.method.as_str() {
            "tools/list" => serde_json::json!({
                "tools": self.tools.values().map(|(t, _)| t).collect::<Vec<_>>(),
            }),
            "tools/call" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let arguments = request
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                match self.tools.get(name) {
                    Some((_tool, handler)) => {
                        let future = handler.call(arguments, None);
                        return match futures::executor::block_on(future) {
                            Ok(HandlerOutcome::Complete(value)) => {
                                ServerOutcome::Respond(JsonRpcResponse::ok(
                                    id,
                                    serde_json::json!({
                                        "content": [{"type": "text", "text": serde_json::to_string(&value).unwrap_or_default()}],
                                        "isError": false,
                                    }),
                                ))
                            }
                            Ok(HandlerOutcome::NeedsInput { .. }) => {
                                ServerOutcome::Respond(JsonRpcResponse::err(
                                    id,
                                    ERROR_INTERNAL,
                                    "legacy protocol does not support multi-round-trip requests",
                                ))
                            }
                            Err(e) => ServerOutcome::Respond(JsonRpcResponse::ok(
                                id,
                                serde_json::json!({
                                    "content": [{"type": "text", "text": e.to_string()}],
                                    "isError": true,
                                }),
                            )),
                        };
                    }
                    None => {
                        return ServerOutcome::Respond(JsonRpcResponse::err(
                            id,
                            ERROR_INVALID_PARAMS,
                            format!("unknown tool: {name}"),
                        ));
                    }
                }
            }
            "resources/list" => serde_json::json!({
                "resources": self.resources.values().map(|(r, _)| r).collect::<Vec<_>>(),
            }),
            "resources/read" => {
                let uri = request
                    .params
                    .get("uri")
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                match self.resources.get(uri) {
                    Some((resource, contents)) => serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": resource.mime_type.clone().unwrap_or_else(|| "text/plain".into()),
                            "text": contents,
                        }]
                    }),
                    None => {
                        return ServerOutcome::Respond(JsonRpcResponse::err(
                            id,
                            ERROR_INVALID_PARAMS,
                            format!("unknown resource: {uri}"),
                        ));
                    }
                }
            }
            "prompts/list" => serde_json::json!({
                "prompts": self.prompts.values().collect::<Vec<_>>(),
            }),
            "prompts/get" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                match self.prompts.get(name) {
                    Some(prompt) => serde_json::json!({
                        "description": prompt.description,
                        "messages": [{
                            "role": "user",
                            "content": {"type": "text", "text": prompt.description.clone().unwrap_or_default()}
                        }],
                    }),
                    None => {
                        return ServerOutcome::Respond(JsonRpcResponse::err(
                            id,
                            ERROR_INVALID_PARAMS,
                            format!("unknown prompt: {name}"),
                        ));
                    }
                }
            }
            "ping" => serde_json::json!({}),
            _ => {
                return ServerOutcome::Respond(JsonRpcResponse::err(
                    id,
                    ERROR_METHOD_NOT_FOUND,
                    format!("method not found: {}", request.method),
                ));
            }
        };
        ServerOutcome::Respond(JsonRpcResponse::ok(id, result))
    }

    /// Handles `tools/call` including MRTR (input responses + request state).
    #[allow(clippy::result_large_err)] // error carries JSON-RPC context
    async fn handle_tool_call(
        &self,
        params: &serde_json::Value,
        client_capabilities: &serde_json::Value,
    ) -> Result<serde_json::Value, JsonRpcResponse> {
        let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let (_tool, handler) = self.tools.get(name).ok_or_else(|| {
            JsonRpcResponse::err(
                serde_json::Value::Null,
                ERROR_INVALID_PARAMS,
                format!("unknown tool: {name}"),
            )
        })?;

        let input =
            if params.get("inputResponses").is_some() || params.get("requestState").is_some() {
                Some(ClientInputs {
                    input_responses: params
                        .get("inputResponses")
                        .cloned()
                        .unwrap_or(serde_json::json!({})),
                    request_state: params
                        .get("requestState")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
            } else {
                None
            };

        match handler.call(arguments, input).await {
            Ok(HandlerOutcome::Complete(value)) => Ok(serde_json::json!({
                "resultType": "complete",
                "content": [{"type": "text", "text": serde_json::to_string(&value).unwrap_or_default()}],
                "structuredContent": value,
                "isError": false
            })),
            Ok(HandlerOutcome::NeedsInput {
                input_requests,
                request_state,
            }) => {
                // The server must not request input the client did not
                // declare (spec: MissingRequiredClientCapabilityError).
                let requests = input_requests.as_object().cloned().unwrap_or_default();
                for request in requests.values() {
                    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
                    let needs_elicitation = method == "elicitation/create";
                    if needs_elicitation {
                        let has_form = client_capabilities
                            .get("elicitation")
                            .and_then(|e| e.get("form"))
                            .is_some();
                        let has_url = client_capabilities
                            .get("elicitation")
                            .and_then(|e| e.get("url"))
                            .is_some();
                        let mode = request
                            .pointer("/params/mode")
                            .and_then(|m| m.as_str())
                            .unwrap_or("form");
                        let supported = if mode == "url" { has_url } else { has_form };
                        if !supported {
                            return Err(JsonRpcResponse::err(
                                serde_json::Value::Null,
                                ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY,
                                "Missing required client capability: elicitation",
                            )
                            .with_error_data(serde_json::json!({
                                "requiredCapabilities": {"elicitation": {}}
                            })));
                        }
                    }
                }
                let mut result = serde_json::json!({
                    "resultType": "input_required",
                    "inputRequests": input_requests,
                });
                if let Some(state) = &request_state {
                    result["requestState"] = serde_json::json!(state);
                }
                Ok(result)
            }
            Err(e) => Ok(serde_json::json!({
                "resultType": "complete",
                "content": [{"type": "text", "text": e.to_string()}],
                "isError": true
            })),
        }
    }

    /// Closes a subscription stream (e.g. on transport end).
    pub fn close_subscription(&self, id: &serde_json::Value) {
        self.subscription_senders
            .lock()
            .unwrap()
            .retain(|(sid, _)| sid != id);
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Test helper: a server with an echo tool (used by transport tests).
#[cfg(test)]
impl McpServer {
    #[cfg(test)]
    pub(crate) fn new_with_http_test_tools() -> McpServer {
        let mut server = McpServer::new();
        server
        .register_tool(
            McpTool::new(
                "echo",
                "echoes the input",
                serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
            ),
            Arc::new(FunctionToolHandler::new(|args| {
                Ok(serde_json::json!({"echo": args.get("text").cloned().unwrap_or(serde_json::Value::Null)}))
            })),
        )
        .unwrap();
        server
    }
}

/// A resolver for `elicitation/create` input requests (client side).
pub type ElicitationResolver =
    Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, AiError> + Send + Sync>;

/// A resolver for `sampling/createMessage` input requests (client side).
pub type SamplingResolver =
    Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, AiError> + Send + Sync>;

/// The modern stateless MCP client over a line-delimited duplex transport.
pub struct McpClient {
    writer: tokio::io::BufWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>,
    reader: tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    next_id: u64,
    protocol_version: String,
    client_info: serde_json::Value,
    client_capabilities: serde_json::Value,
    elicitation: Option<ElicitationResolver>,
    sampling: Option<SamplingResolver>,
    /// Maximum MRTR retry rounds for one request.
    max_rounds: u32,
    /// Legacy (initialize-handshake) mode: no `_meta`, no `resultType`.
    legacy: bool,
    /// Whether the legacy handshake has completed.
    legacy_ready: bool,
}

impl McpClient {
    pub fn new(duplex: tokio::io::DuplexStream) -> Self {
        let (reader, writer) = tokio::io::split(duplex);
        Self {
            writer: tokio::io::BufWriter::new(writer),
            reader: tokio::io::BufReader::new(reader),
            next_id: 1,
            protocol_version: PROTOCOL_VERSION_2026_07_28.to_string(),
            client_info: serde_json::json!({"name": "ai-sdk-mcp-client", "version": "0.2.0"}),
            client_capabilities: serde_json::json!({"elicitation": {"form": {}}}),
            elicitation: None,
            sampling: None,
            max_rounds: 4,
            legacy: false,
            legacy_ready: false,
        }
    }

    pub fn with_protocol_version(mut self, version: &str) -> Self {
        self.protocol_version = version.to_string();
        self
    }

    /// Switches the client to the legacy (2025-11-25 initialize-handshake)
    /// dialect: no per-request `_meta`, results without `resultType`.
    pub fn with_legacy(mut self) -> Self {
        self.legacy = true;
        self
    }

    pub fn with_elicitation_resolver(mut self, resolver: ElicitationResolver) -> Self {
        self.elicitation = Some(resolver);
        self
    }

    pub fn with_sampling_resolver(mut self, resolver: SamplingResolver) -> Self {
        self.sampling = Some(resolver);
        self
    }

    fn next_id(&mut self) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        serde_json::json!(id)
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

    async fn write_request(&mut self, request: &JsonRpcRequest) -> Result<(), AiError> {
        use tokio::io::AsyncWriteExt;
        let mut line = serde_json::to_string(request)
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))?;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| ai_errors::NetworkError::new("mcp write", e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| ai_errors::NetworkError::new("mcp flush", e.to_string()))?;
        Ok(())
    }

    async fn read_response(&mut self, id: &serde_json::Value) -> Result<JsonRpcResponse, AiError> {
        use tokio::io::AsyncBufReadExt;
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| ai_errors::NetworkError::new("mcp read", e.to_string()))?;
            if n == 0 {
                return Err(AiError::Network(ai_errors::NetworkError::new(
                    "mcp",
                    "server closed the connection",
                )));
            }
            if line.trim().is_empty() {
                continue;
            }
            let response: JsonRpcResponse = serde_json::from_str(line.trim())
                .map_err(|e| SerializationError::new(format!("invalid response: {e}")))?;
            if response.id == *id {
                return Ok(response);
            }
            // Ignore notifications and responses to other ids.
        }
    }

    /// Calls a method with the MRTR loop: fulfills `input_required`
    /// results and retries with `inputResponses` + `requestState`.
    pub async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        if self.legacy {
            return self.call_legacy(method, params).await;
        }
        let mut params = self.build_params(params);
        let mut round = 0u32;

        loop {
            let id = self.next_id();
            let request = JsonRpcRequest::new(id.clone(), method, params.clone());
            self.write_request(&request).await?;
            let response = self.read_response(&id).await?;

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
                        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
                        let request_params = request
                            .get("params")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        let response_value = match method {
                            "elicitation/create" => {
                                let resolver = self.elicitation.clone().ok_or_else(|| {
                                    ai_errors::InternalError::new(
                                        "server requested elicitation but no elicitation resolver is configured",
                                    )
                                })?;
                                resolver(request_params)?
                            }
                            "sampling/createMessage" => {
                                let resolver = self.sampling.clone().ok_or_else(|| {
                                    ai_errors::InternalError::new(
                                        "server requested sampling but no sampling resolver is configured",
                                    )
                                })?;
                                resolver(request_params)?
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

    /// Performs the legacy initialize handshake (one request per
    /// connection, per the 2025-11-25 semantics).
    async fn legacy_handshake(&mut self) -> Result<(), AiError> {
        let initialize = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            serde_json::json!({
                "protocolVersion": LEGACY_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": self.client_info,
            }),
        );
        self.write_request(&initialize).await?;
        let id = initialize.id.clone();
        let response = self.read_response(&id).await?;
        if let Some(error) = response.error {
            return Err(map_mcp_error(error));
        }
        // Notify the server that initialization completed (legacy
        // notification; no id, no response).
        let notification =
            JsonRpcNotification::new("notifications/initialized", serde_json::json!({}));
        let mut line = serde_json::to_string(&notification)
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))?;
        line.push('\n');
        use tokio::io::AsyncWriteExt;
        self.writer.write_all(line.as_bytes()).await.map_err(|e| {
            AiError::Network(ai_errors::NetworkError::new("mcp write", e.to_string()))
        })?;
        self.writer.flush().await.map_err(|e| {
            AiError::Network(ai_errors::NetworkError::new("mcp flush", e.to_string()))
        })?;
        self.legacy_ready = true;
        Ok(())
    }

    /// True once the legacy initialize handshake has completed.
    pub fn in_legacy_handshake_done(&self) -> bool {
        self.legacy_ready
    }

    /// Calls a method in legacy mode: no `_meta`, no `resultType`, no MRTR.
    async fn call_legacy(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        if !self.legacy_ready {
            self.legacy_handshake().await?;
        }
        let id = self.next_id();
        let request = JsonRpcRequest::new(id.clone(), method, params);
        self.write_request(&request).await?;
        let response = self.read_response(&id).await?;
        if let Some(error) = response.error {
            return Err(map_mcp_error(error));
        }
        response.result.ok_or_else(|| {
            AiError::Internal(ai_errors::InternalError::new(
                "legacy response without result",
            ))
        })
    }

    /// Calls `server/discover` and returns the supported versions.
    pub async fn discover(&mut self) -> Result<Vec<String>, AiError> {
        let result = self.call("server/discover", serde_json::json!({})).await?;
        let versions = result
            .get("supportedVersions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(versions)
    }

    /// Picks a mutually supported version; retries `discover` if the current
    /// version is rejected with `-32022`.
    pub async fn discover_and_negotiate(&mut self) -> Result<Vec<String>, AiError> {
        match self.discover().await {
            Ok(versions) => Ok(versions),
            Err(e) => {
                if let Some(supported) = unsupported_versions(&e) {
                    if let Some(best) = supported
                        .iter()
                        .find(|v| v.as_str() == PROTOCOL_VERSION_2026_07_28)
                    {
                        self.protocol_version = best.clone();
                        self.discover().await
                    } else {
                        Err(e)
                    }
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, AiError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params = serde_json::json!({});
            if let Some(cursor) = &cursor {
                params["cursor"] = serde_json::json!(cursor);
            }
            let result = self.call("tools/list", params).await?;
            let page: Vec<McpTool> = parse_list(&result, "tools")?;
            all.extend(page);
            match result.get("nextCursor").and_then(|c| c.as_str()) {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(all)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        self.call(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        )
        .await
    }

    pub async fn list_resources(&mut self) -> Result<Vec<McpResource>, AiError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params = serde_json::json!({});
            if let Some(cursor) = &cursor {
                params["cursor"] = serde_json::json!(cursor);
            }
            let result = self.call("resources/list", params).await?;
            let page: Vec<McpResource> = parse_list(&result, "resources")?;
            all.extend(page);
            match result.get("nextCursor").and_then(|c| c.as_str()) {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(all)
    }

    pub async fn read_resource(&mut self, uri: &str) -> Result<String, AiError> {
        let result = self
            .call("resources/read", serde_json::json!({"uri": uri}))
            .await?;
        Ok(result
            .pointer("/contents/0/text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string())
    }

    pub async fn list_prompts(&mut self) -> Result<Vec<McpPrompt>, AiError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params = serde_json::json!({});
            if let Some(cursor) = &cursor {
                params["cursor"] = serde_json::json!(cursor);
            }
            let result = self.call("prompts/list", params).await?;
            let page: Vec<McpPrompt> = parse_list(&result, "prompts")?;
            all.extend(page);
            match result.get("nextCursor").and_then(|c| c.as_str()) {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(all)
    }
}

pub fn parse_list<T: serde::de::DeserializeOwned>(
    result: &serde_json::Value,
    field: &str,
) -> Result<Vec<T>, AiError> {
    serde_json::from_value(
        result
            .get(field)
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])),
    )
    .map_err(|e| {
        AiError::Serialization(SerializationError::new(format!(
            "invalid `{field}` list: {e}"
        )))
    })
}

/// Extracts `data.supported` from an `UnsupportedProtocolVersionError`.
pub fn unsupported_versions(error: &AiError) -> Option<Vec<String>> {
    let text = error.to_string();
    // The error carries data.supported in the message when constructed by
    // map_mcp_error; parse the embedded JSON.
    let start = text.find("[")?;
    let end = text.rfind(']')?;
    if start >= end {
        return None;
    }
    let list: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    list.as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    })
}

/// Maps an MCP JSON-RPC error to a typed [`AiError`] with context.
pub fn map_mcp_error(error: crate::jsonrpc::JsonRpcError) -> AiError {
    let message = error.to_string();
    match error.code {
        ERROR_UNSUPPORTED_PROTOCOL_VERSION => {
            // Embed the supported list as JSON so clients can parse and
            // retry with a mutually supported version.
            let supported_json = serde_json::to_string(
                error
                    .data
                    .get("supported")
                    .unwrap_or(&serde_json::Value::Null),
            )
            .unwrap_or_else(|_| "null".to_string());
            AiError::Internal(ai_errors::InternalError::new(format!(
                "MCP unsupported protocol version: {message} (supported: {supported_json})"
            )))
        }
        ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY => {
            AiError::Internal(ai_errors::InternalError::new(format!(
                "MCP missing required client capability: {message} (required: {:?})",
                error.data.get("requiredCapabilities")
            )))
        }
        ERROR_HEADER_MISMATCH => AiError::Internal(ai_errors::InternalError::new(format!(
            "MCP header mismatch: {message}"
        ))),
        _ => AiError::Internal(ai_errors::InternalError::new(format!(
            "MCP error: {message}"
        ))),
    }
}

// Builder helpers for JSON-RPC error responses with data.
impl JsonRpcResponse {
    fn with_error_data(mut self, data: serde_json::Value) -> Self {
        if let Some(error) = &mut self.error {
            error.data = data;
        }
        self
    }

    fn with_id(mut self, id: serde_json::Value) -> Self {
        self.id = id;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a server over one end of a duplex pipe and returns the client
    /// for the other end.
    fn spawn_server(server: McpServer) -> McpClient {
        let (client_side, server_side) = tokio::io::duplex(65536);
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            let (reader, writer) = tokio::io::split(server_side);
            let mut reader = tokio::io::BufReader::new(reader);
            let mut writer = tokio::io::BufWriter::new(writer);
            let mut line = String::new();
            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                eprintln!("[srv] line: {:?}", line);
                if line.trim().is_empty() {
                    line.clear();
                    continue;
                }
                // Skip notifications (no id): legacy `initialized` and
                // modern `notifications/*`.
                let parsed: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
                if parsed.get("id").is_none() {
                    line.clear();
                    continue;
                }
                let request: JsonRpcRequest = serde_json::from_value(parsed).unwrap();
                match server.handle_request(&request).await {
                    ServerOutcome::Respond(response) => {
                        writer
                            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                            .await
                            .unwrap();
                        writer.write_all(b"\n").await.unwrap();
                        // Flush: the loop stays alive, so the response must
                        // be pushed into the pipe now (drop-flush would never
                        // happen).
                        writer.flush().await.unwrap();
                    }
                    ServerOutcome::Subscription(mut handle) => {
                        // Deliver notifications inline (the listen stream is
                        // the last request in these tests): write one
                        // notification, then the graceful end result.
                        if let Some(notification) = handle.receiver.recv().await {
                            eprintln!("[srv] got notification {}", notification.method);
                            let mut params = notification.params;
                            if let Some(obj) = params.as_object_mut() {
                                let meta = obj
                                    .entry("_meta".to_string())
                                    .or_insert_with(|| serde_json::json!({}));
                                if let Some(meta_obj) = meta.as_object_mut() {
                                    meta_obj.insert(
                                        META_SUBSCRIPTION_ID.to_string(),
                                        handle.id.clone(),
                                    );
                                }
                            }
                            let mut notification_line = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": notification.method,
                                "params": params
                            })
                            .to_string();
                            notification_line.push('\n');
                            writer
                                .write_all(notification_line.as_bytes())
                                .await
                                .unwrap();
                        }
                        let result = serde_json::json!({
                            "resultType": "complete",
                            "_meta": {META_SUBSCRIPTION_ID: handle.id}
                        });
                        let response = JsonRpcResponse::ok(handle.id, result);
                        writer
                            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                            .await
                            .unwrap();
                        writer.write_all(b"\n").await.unwrap();
                    }
                }
                line.clear();
            }
        });
        McpClient::new(client_side)
    }

    fn echo_server() -> McpServer {
        let mut server = McpServer::new();
        server
            .register_tool(
                McpTool::new(
                    "echo",
                    "echoes the input",
                    serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
                ),
                Arc::new(FunctionToolHandler::new(|args| {
                    Ok(serde_json::json!({"echo": args.get("text").cloned().unwrap_or(serde_json::Value::Null)}))
                })),
            )
            .unwrap();
        server.register_resource(
            McpResource {
                uri: "docs://guide".into(),
                name: "Guide".into(),
                description: Some("usage guide".into()),
                mime_type: Some("text/markdown".into()),
                size: None,
            },
            "# Guide\nhello",
        );
        server.register_prompt(McpPrompt {
            name: "greet".into(),
            description: Some("Greets the user".into()),
            arguments: vec![],
        });
        server
    }

    #[tokio::test]
    async fn discover_returns_modern_result() {
        let mut client = spawn_server(echo_server());
        let versions = client.discover().await.unwrap();
        assert_eq!(versions, vec![PROTOCOL_VERSION_2026_07_28.to_string()]);
    }

    #[tokio::test]
    async fn missing_meta_is_rejected_with_invalid_params() {
        let (client_side, server_side) = tokio::io::duplex(4096);
        let server_task = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            let (reader, writer) = tokio::io::split(server_side);
            let mut reader = tokio::io::BufReader::new(reader);
            let mut writer = tokio::io::BufWriter::new(writer);
            let server = McpServer::new();
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: JsonRpcRequest = serde_json::from_str(line.trim()).unwrap();
            match server.handle_request(&request).await {
                ServerOutcome::Respond(response) => {
                    writer
                        .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                        .await
                        .unwrap();
                    writer.write_all(b"\n").await.unwrap();
                    // Explicit flush: the response must reach the client
                    // before the task ends (drop-flush is not guaranteed to
                    // complete before the peer observes EOF).
                    writer.flush().await.unwrap();
                }
                _ => panic!("expected a response"),
            }
        });

        // Raw request WITHOUT _meta — must be rejected with -32602.
        let mut client = McpClient::new(client_side);
        let id = serde_json::json!(1);
        let request = JsonRpcRequest::new(id.clone(), "tools/list", serde_json::json!({}));
        // Write manually to bypass the client's _meta injection.
        use tokio::io::AsyncWriteExt;
        let mut line = serde_json::to_string(&request).unwrap();
        line.push('\n');
        client.writer.write_all(line.as_bytes()).await.unwrap();
        client.writer.flush().await.unwrap();
        let response = client.read_response(&id).await.unwrap();
        let error = response.error.expect("missing _meta must error");
        assert_eq!(error.code, ERROR_INVALID_PARAMS);
        assert!(
            error.message.contains("protocolVersion"),
            "{}",
            error.message
        );
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn unsupported_version_returns_32022_with_supported_list() {
        let mut client = spawn_server(echo_server()).with_protocol_version("2025-11-25");
        let err = client.list_tools().await.unwrap_err();
        assert!(
            err.to_string().contains("Unsupported protocol version"),
            "{err}"
        );
        let supported = unsupported_versions(&err).unwrap_or_default();
        assert!(
            supported.contains(&PROTOCOL_VERSION_2026_07_28.to_string()),
            "{supported:?}"
        );
    }

    #[tokio::test]
    async fn version_retry_negotiates_supported_version() {
        let mut client = spawn_server(echo_server()).with_protocol_version("2025-11-25");
        let versions = client.discover_and_negotiate().await.unwrap();
        assert!(versions.contains(&PROTOCOL_VERSION_2026_07_28.to_string()));
        // After negotiation the client speaks the supported version.
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn modern_roundtrip_tools_resources_prompts() {
        let mut client = spawn_server(echo_server());
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert!(tools[0].input_schema.get("type") == Some(&serde_json::json!("object")));

        let result = client
            .call_tool("echo", serde_json::json!({"text": "hi"}))
            .await
            .unwrap();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["content"][0]["text"], "{\"echo\":\"hi\"}");
        assert_eq!(result["structuredContent"]["echo"], "hi");

        let resources = client.list_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(
            client.read_resource("docs://guide").await.unwrap(),
            "# Guide\nhello"
        );

        let prompts = client.list_prompts().await.unwrap();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "greet");
    }

    #[tokio::test]
    async fn cursor_pagination_returns_all_items() {
        let mut server = McpServer::new();
        for i in 0..7 {
            server
                .register_tool(
                    McpTool::new(
                        format!("tool_{i}"),
                        format!("tool {i}"),
                        serde_json::json!({"type": "object", "properties": {}}),
                    ),
                    Arc::new(FunctionToolHandler::new(|_| Ok(serde_json::json!({})))),
                )
                .unwrap();
        }
        let mut client = spawn_server(server);

        // list_tools auto-paginates through every page (page size 100 on
        // the server; to exercise the cursor path we call the paginated
        // method directly with a manual cursor).
        let all = client.list_tools().await.unwrap();
        assert_eq!(all.len(), 7);

        // Explicit cursor round-trip through the raw call.
        let page1 = client
            .call("tools/list", serde_json::json!({}))
            .await
            .unwrap();
        let tools1 = page1
            .get("tools")
            .and_then(|t| t.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let cursor = page1
            .get("nextCursor")
            .and_then(|c| c.as_str())
            .map(String::from);
        assert!(tools1 > 0);
        if let Some(cursor) = cursor {
            let page2 = client
                .call("tools/list", serde_json::json!({"cursor": cursor}))
                .await
                .unwrap();
            assert!(page2.get("tools").is_some());
        }
    }

    #[tokio::test]
    async fn legacy_handshake_roundtrip_on_dual_era_server() {
        let server = echo_server().enable_legacy(&["2025-11-25"]);
        let mut client = spawn_server(server).with_legacy();

        // Legacy initialize + initialized notification, then plain
        // tools/list without _meta and without resultType.
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        // A legacy tool call works without resultType.
        let result = client
            .call_tool("echo", serde_json::json!({"text": "legacy"}))
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "{\"echo\":\"legacy\"}");
    }

    #[tokio::test]
    async fn modern_and_legacy_clients_coexist() {
        // Legacy client on one connection.
        let mut legacy = spawn_server(echo_server().enable_legacy(&["2025-11-25"])).with_legacy();
        let legacy_tools = legacy.list_tools().await.unwrap();
        assert_eq!(legacy_tools.len(), 1);

        // Modern client on a fresh connection still gets resultType.
        let mut modern = spawn_server(echo_server().enable_legacy(&["2025-11-25"]));
        let tools = modern.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        let result = modern
            .call_tool("echo", serde_json::json!({"text": "modern"}))
            .await
            .unwrap();
        assert_eq!(result["resultType"], "complete");
    }

    #[tokio::test]
    async fn mrrr_elicitation_roundtrip() {
        // A tool that requires the user's name before it can complete.
        let mut server = McpServer::new();
        server
            .register_tool(
                McpTool::new(
                    "personalize",
                    "greets by name",
                    serde_json::json!({"type": "object", "properties": {}}),
                ),
                Arc::new(MrrrToolHandler),
            )
            .unwrap();
        let mut client = spawn_server(server).with_elicitation_resolver(Arc::new(|params| {
            assert_eq!(params["mode"], "form");
            assert_eq!(params["message"], "What is your name?");
            Ok(serde_json::json!({"action": "accept", "content": {"name": "Ada"}}))
        }));

        let result = client
            .call_tool("personalize", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["structuredContent"]["greeting"], "Hello, Ada");
    }

    /// Handler: first call requests a name via elicitation; the retry
    /// completes using the provided input.
    struct MrrrToolHandler;

    #[async_trait]
    impl McpToolHandler for MrrrToolHandler {
        async fn call(
            &self,
            _arguments: serde_json::Value,
            input: Option<ClientInputs>,
        ) -> Result<HandlerOutcome, AiError> {
            match input {
                None => {
                    let (key, request) = elicitation_form_input_request(
                        "name",
                        "What is your name?",
                        serde_json::json!({
                            "type": "object",
                            "properties": {"name": {"type": "string"}},
                            "required": ["name"]
                        }),
                    );
                    Ok(HandlerOutcome::NeedsInput {
                        input_requests: serde_json::json!({key: request}),
                        request_state: Some("personalize-v1".into()),
                    })
                }
                Some(input) => {
                    assert_eq!(input.request_state.as_deref(), Some("personalize-v1"));
                    let name = input.input_responses["name"]["content"]["name"]
                        .as_str()
                        .unwrap_or("?");
                    Ok(HandlerOutcome::Complete(serde_json::json!({
                        "greeting": format!("Hello, {name}")
                    })))
                }
            }
        }
    }

    #[tokio::test]
    async fn elicitation_without_client_capability_is_rejected() {
        // Server requires elicitation; client does NOT declare the
        // capability → -32021 with requiredCapabilities.
        let mut server = McpServer::new();
        server
            .register_tool(
                McpTool::new(
                    "personalize",
                    "greets by name",
                    serde_json::json!({"type": "object", "properties": {}}),
                ),
                Arc::new(MrrrToolHandler),
            )
            .unwrap();
        let mut client = spawn_server(server);
        client.client_capabilities = serde_json::json!({}); // no elicitation
        let err = client
            .call_tool("personalize", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("Missing required client capability"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn subscriptions_deliver_list_changed_notifications() {
        let (client_side, server_side) = tokio::io::duplex(65536);
        let server = Arc::new(tokio::sync::Mutex::new(echo_server()));
        let server_for_spawn = server.clone();
        let (established_tx, established_rx) = tokio::sync::oneshot::channel();
        let mut established_tx = Some(established_tx);
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            let (reader, writer) = tokio::io::split(server_side);
            let mut reader = tokio::io::BufReader::new(reader);
            let mut writer = tokio::io::BufWriter::new(writer);
            let mut line = String::new();
            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                if line.trim().is_empty() {
                    line.clear();
                    continue;
                }
                let request: JsonRpcRequest = serde_json::from_str(line.trim()).unwrap();
                let outcome = {
                    let guard = server_for_spawn.lock().await;
                    guard.handle_request(&request).await
                };
                match outcome {
                    ServerOutcome::Respond(response) => {
                        writer
                            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                            .await
                            .unwrap();
                        writer
                            .write_all(
                                b"
",
                            )
                            .await
                            .unwrap();
                        writer.flush().await.unwrap();
                    }
                    ServerOutcome::Subscription(mut handle) => {
                        // Signal that the subscription is registered, then
                        // deliver one notification and end the stream.
                        if let Some(tx) = established_tx.take() {
                            let _ = tx.send(());
                        }
                        if let Some(notification) = handle.receiver.recv().await {
                            let mut params = notification.params;
                            if let Some(obj) = params.as_object_mut() {
                                obj.insert(
                                    "_meta".into(),
                                    serde_json::json!({META_SUBSCRIPTION_ID: handle.id.clone()}),
                                );
                            }
                            let mut out = serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": notification.method,
                                "params": params
                            })
                            .to_string();
                            out.push('\n');
                            writer.write_all(out.as_bytes()).await.unwrap();
                            writer.flush().await.unwrap();
                        }
                        // Graceful end: respond with a result.
                        let result = serde_json::json!({
                            "resultType": "complete",
                            "_meta": {META_SUBSCRIPTION_ID: handle.id}
                        });
                        let response = JsonRpcResponse::ok(handle.id, result);
                        let mut out = serde_json::to_string(&response).unwrap();
                        out.push('\n');
                        writer.write_all(out.as_bytes()).await.unwrap();
                        writer.flush().await.unwrap();
                    }
                }
                line.clear();
            }
        });

        // Client: open a listen stream, wait until the server confirms the
        // subscription is registered, then trigger a notification.
        let mut client = McpClient::new(client_side);
        let id = client.next_id();
        let params = client.build_params(serde_json::json!({
            "notifications": {"toolsListChanged": true}
        }));
        let request = JsonRpcRequest::new(id.clone(), "subscriptions/listen", params);
        client.write_request(&request).await.unwrap();
        established_rx.await.expect("subscription established");

        // Register a tool → server broadcasts tools/list_changed.
        server
            .lock()
            .await
            .register_tool(
                McpTool::new(
                    "new_tool",
                    "added later",
                    serde_json::json!({"type": "object"}),
                ),
                Arc::new(FunctionToolHandler::new(|_| Ok(serde_json::json!({})))),
            )
            .unwrap();

        // Read the notification line (no id) then the closing response.
        use tokio::io::AsyncBufReadExt;
        let mut notification_line = String::new();
        client
            .reader
            .read_line(&mut notification_line)
            .await
            .unwrap();
        let notification: JsonRpcNotification =
            serde_json::from_str(notification_line.trim()).unwrap();
        assert_eq!(notification.method, "notifications/tools/list_changed");
        let meta = notification
            .params
            .get("_meta")
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            meta.get(META_SUBSCRIPTION_ID),
            Some(&id),
            "notification carries subscriptionId"
        );

        let response = client.read_response(&id).await.unwrap();
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap()["resultType"], "complete");
    }
}
