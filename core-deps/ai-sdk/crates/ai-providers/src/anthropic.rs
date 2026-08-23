//! Anthropic native adapter (Messages API): real wire format with
//! `x-api-key` + `anthropic-version` headers, tool use blocks, prompt
//! caching passthrough, and SSE streaming via `content_block_*` events.
//!
//! Requires `ANTHROPIC_API_KEY`. Untested live without credentials
//! (documented); unit tests cover the wire shapes.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};

use ai_core::{ChatRequest, EventStream, Model, Provider, ResponseFormat};
use ai_errors::{AiError, SerializationError};
use ai_models::{ModelCapabilities, ModelInfo};
use ai_types::{
    Completion, ContentPart, Message, ModelId, ProviderId, Role, StreamEvent, ToolCall, Usage,
};

use ai_stream::sse_parse;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};

/// The Anthropic API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The Messages API path.
pub const MESSAGES_PATH: &str = "v1/messages";

/// Configuration for the Anthropic provider.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout: Duration,
}

impl AnthropicConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com".to_string(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn from_provider_config(cfg: &ai_config::ProviderConfig) -> Result<Self, AiError> {
        let api_key = cfg.require_api_key("anthropic")?.to_string();
        let base_url = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());
        Ok(Self {
            api_key,
            base_url,
            timeout: Duration::from_secs(60),
        })
    }
}

/// The Anthropic provider (real HTTP adapter).
pub struct AnthropicProvider {
    config: AnthropicConfig,
    http: HttpClient,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("base_url", &self.config.base_url)
            .field("api_key", &"***redacted***")
            .finish()
    }
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Result<Self, AiError> {
        Ok(Self {
            config,
            http: HttpClient::shared(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.config.base_url.trim_end_matches('/'), path)
    }

    async fn request_json(&self, body: Value) -> Result<Value, AiError> {
        let response = tokio::time::timeout(
            self.config.timeout,
            self.http.execute(
                self.http
                    .post(self.url(MESSAGES_PATH))
                    .header("x-api-key", &self.config.api_key)
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("content-type", "application/json")
                    .json(&body),
            ),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "anthropic.messages",
                self.config.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("anthropic.messages", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("anthropic.messages", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("anthropic", status, retry_after, &bytes).await);
        }
        parse_json("anthropic.messages", &bytes)
    }

    async fn request_stream(&self, body: Value) -> Result<EventStream, AiError> {
        let response = tokio::time::timeout(
            self.config.timeout,
            self.http.execute(
                self.http
                    .post(self.url(MESSAGES_PATH))
                    .header("x-api-key", &self.config.api_key)
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("content-type", "application/json")
                    .json(&body),
            ),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "anthropic.messages",
                self.config.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("anthropic.messages", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| map_reqwest_error("anthropic.messages", e))?
                .to_vec();
            return Err(map_response_error("anthropic", status, retry_after, &bytes).await);
        }

        let operation = "anthropic.messages".to_string();
        let byte_stream = response
            .bytes_stream()
            .map(move |item| item.map_err(|e| map_reqwest_error(&operation, e)));
        Ok(map_anthropic_sse(sse_parse(byte_stream)))
    }
}

/// Serializes a unified request into the Anthropic Messages wire format.
fn build_messages_body(request: &ChatRequest, model: &str) -> Result<Value, AiError> {
    let mut system: Vec<String> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    let mut pending_tool_results: Vec<Value> = Vec::new();

    for message in &request.messages {
        match message.role {
            Role::System => system.push(message.text_content()),
            Role::User => {
                // Flush any tool results accumulated before this user turn
                // (Anthropic requires tool_result blocks in user messages).
                let mut content: Vec<Value> = std::mem::take(&mut pending_tool_results);
                match serialize_user_content(message)? {
                    Value::Array(blocks) => content.extend(blocks),
                    other => content.push(other),
                }
                messages.push(json!({"role": "user", "content": content}));
            }
            Role::Assistant => {
                let mut blocks: Vec<Value> = Vec::new();
                let text = message.text_content();
                if !text.is_empty() {
                    blocks.push(json!({"type": "text", "text": text}));
                }
                for part in &message.parts {
                    if let ContentPart::ToolCall { call } = part {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": parse_json_args(&call.arguments),
                        }));
                    }
                }
                messages.push(json!({"role": "assistant", "content": blocks}));
            }
            Role::Tool => {
                for part in &message.parts {
                    if let ContentPart::ToolResult { result } = part {
                        pending_tool_results.push(json!({
                            "type": "tool_result",
                            "tool_use_id": result.id,
                            "content": parse_json_args(&result.output),
                            "is_error": result.is_error,
                        }));
                    }
                }
            }
        }
    }

    // Trailing tool results (no following user message yet).
    if !pending_tool_results.is_empty() {
        messages.push(json!({"role": "user", "content": pending_tool_results}));
    }

    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = Value::Array(system.into_iter().map(Value::String).collect());
    }
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        body["tools"] = Value::Array(tools);
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if matches!(
        request.response_format,
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. }
    ) {
        // Anthropic: instruct plain JSON output in the system prompt.
        let system_arr = body
            .get("system")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        let mut parts = system_arr;
        parts.push(Value::String(
            "Respond with valid JSON only, no markdown fences.".to_string(),
        ));
        body["system"] = Value::Array(parts);
    }
    if let Some(extra) = request.provider_options.as_object() {
        for (key, value) in extra {
            body[key] = value.clone();
        }
    }
    Ok(body)
}

fn parse_json_args(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| Value::String(arguments.to_string()))
}

fn serialize_user_content(message: &Message) -> Result<Value, AiError> {
    let text = message.text_content();
    let mut blocks: Vec<Value> = Vec::new();
    if !text.is_empty() {
        blocks.push(json!({"type": "text", "text": text}));
    }
    for part in &message.parts {
        if let ContentPart::Image { image } = part {
            let source = match image {
                ai_types::ImageSource::Url { url } => json!({"type": "url", "url": url}),
                ai_types::ImageSource::Base64 { media_type, data } => {
                    json!({"type": "base64", "media_type": media_type, "data": data})
                }
            };
            blocks.push(json!({"type": "image", "source": source}));
        }
    }
    if blocks.is_empty() {
        return Err(AiError::Serialization(SerializationError::new(
            "user message has no serializable content",
        )));
    }
    Ok(Value::Array(blocks))
}

/// Parses a non-streaming Messages response.
fn parse_messages_response(
    provider: &ProviderId,
    model: &ModelId,
    json: &Value,
) -> Result<Completion, AiError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut reasoning = None;

    if let Some(content) = json.get("content").and_then(|c| c.as_array()) {
        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("thinking") => {
                    if let Some(t) = block.get("thinking").and_then(|t| t.as_str()) {
                        reasoning = Some(t.to_string());
                    }
                }
                Some("tool_use") => {
                    let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    tool_calls.push(ToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments: serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
                    });
                }
                _ => {}
            }
        }
    }

    let usage = Usage {
        input_tokens: json
            .pointer("/usage/input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: json
            .pointer("/usage/output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        // Anthropic does not expose reasoning tokens separately in usage.
        reasoning_tokens: None,
        cached_input_tokens: json
            .pointer("/usage/cache_read_input_tokens")
            .and_then(|v| v.as_u64()),
        total_tokens: None,
    };

    let finish_reason = json
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(Completion {
        provider: provider.clone(),
        model: model.clone(),
        text,
        tool_calls,
        usage,
        reasoning,
        raw: json.clone(),
        finish_reason,
    })
}

/// Converts Anthropic SSE events into unified stream events.
fn map_anthropic_sse(sse: ai_stream::SseStream) -> EventStream {
    // In-flight tool call being streamed: (id, name, accumulated args).
    let mut in_flight_tool: Option<(String, String, String)> = None;

    let stream = sse.flat_map(move |sse_result| {
        let mut out: Vec<Result<StreamEvent, AiError>> = Vec::new();
        match sse_result {
            Err(e) => out.push(Err(e)),
            Ok(event) => {
                let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
                    out.push(Err(AiError::Serialization(SerializationError::new(
                        format!(
                            "invalid Anthropic SSE payload: {}",
                            &event.data[..event.data.len().min(200)]
                        ),
                    ))));
                    return futures::stream::iter(out);
                };
                let event_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match event_type {
                    "content_block_start" => {
                        let block = payload.get("content_block").cloned().unwrap_or(Value::Null);
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            in_flight_tool =
                                Some((id.to_string(), name.to_string(), String::new()));
                            out.push(Ok(StreamEvent::ToolCallStarted {
                                id: id.to_string(),
                                name: name.to_string(),
                            }));
                        }
                    }
                    "content_block_delta" => {
                        let delta = payload.get("delta").cloned().unwrap_or(Value::Null);
                        match delta.get("type").and_then(|t| t.as_str()) {
                            Some("text_delta") => {
                                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                    out.push(Ok(StreamEvent::TextDelta {
                                        delta: text.to_string(),
                                    }));
                                }
                            }
                            Some("thinking_delta") => {
                                if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                                    out.push(Ok(StreamEvent::ReasoningDelta {
                                        delta: text.to_string(),
                                    }));
                                }
                            }
                            Some("input_json_delta") => {
                                if let Some(partial) =
                                    delta.get("partial_json").and_then(|t| t.as_str())
                                {
                                    if let Some((id, _name, args)) = in_flight_tool.as_mut() {
                                        args.push_str(partial);
                                        out.push(Ok(StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_delta: partial.to_string(),
                                        }));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_delta" => {
                        if let Some(reason) = payload
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|v| v.as_str())
                        {
                            if let Some((id, name, arguments)) = in_flight_tool.take() {
                                out.push(Ok(StreamEvent::ToolCallCompleted {
                                    call: ai_types::ToolCall {
                                        id,
                                        name,
                                        arguments,
                                    },
                                }));
                            }
                            out.push(Ok(StreamEvent::Completed {
                                finish_reason: Some(reason.to_string()),
                            }));
                        }
                    }
                    "message_start" => {
                        if let Some(usage) = payload.pointer("/message/usage") {
                            out.push(Ok(StreamEvent::UsageUpdate {
                                usage: Usage {
                                    input_tokens: usage
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0),
                                    output_tokens: usage
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0),
                                    reasoning_tokens: None,
                                    cached_input_tokens: usage
                                        .get("cache_read_input_tokens")
                                        .and_then(|v| v.as_u64()),
                                    total_tokens: None,
                                },
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
        futures::stream::iter(out)
    });
    Box::pin(stream)
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError> {
        // Anthropic exposes a curated model list; without a documented
        // public listing endpoint we return the known catalog entries for
        // this provider.
        let mut models = Vec::new();
        for id in [
            "claude-sonnet-4-20250514",
            "claude-3-5-sonnet-20241022",
            "claude-3-opus-20240229",
            "claude-3-haiku-20240307",
        ] {
            models.push(
                ModelInfo::new(
                    ProviderId::new("anthropic"),
                    ModelId::new(id),
                    200_000,
                    64_000,
                )
                .with_name(id)
                .with_capabilities(ModelCapabilities {
                    input_modalities: vec![ai_types::Modality::Text, ai_types::Modality::Image],
                    output_modalities: vec![ai_types::Modality::Text],
                    supports_streaming: true,
                    supports_tools: true,
                    supports_structured_output: false,
                    supports_embeddings: false,
                    supports_vision: true,
                    supports_fine_tuning: false,
                }),
            );
        }
        Ok(models)
    }

    fn model(&self, model_id: &str) -> Result<Arc<dyn Model>, AiError> {
        let info = ModelInfo::new(
            ProviderId::new("anthropic"),
            ModelId::new(model_id),
            200_000,
            64_000,
        )
        .with_name(model_id)
        .with_capabilities(ModelCapabilities {
            input_modalities: vec![ai_types::Modality::Text, ai_types::Modality::Image],
            output_modalities: vec![ai_types::Modality::Text],
            supports_streaming: true,
            supports_tools: true,
            supports_structured_output: false,
            supports_embeddings: false,
            supports_vision: true,
            supports_fine_tuning: false,
        });
        Ok(Arc::new(AnthropicModel {
            provider: Arc::new(self.clone_for_model()),
            info,
        }))
    }
}

impl AnthropicProvider {
    fn clone_for_model(&self) -> Self {
        Self {
            config: self.config.clone(),
            http: self.http.clone(),
        }
    }
}

/// A model behind the Anthropic provider.
pub struct AnthropicModel {
    provider: Arc<AnthropicProvider>,
    info: ModelInfo,
}

#[async_trait]
impl Model for AnthropicModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let body = build_messages_body(&request, self.info.id.as_str())?;
        let json = self.provider.request_json(body).await?;
        parse_messages_response(&self.info.provider, &self.info.id, &json)
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, AiError> {
        let mut body = build_messages_body(&request, self.info.id.as_str())?;
        body["stream"] = json!(true);
        self.provider.request_stream(body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_tool_calls_and_results() {
        let request = ChatRequest::new(vec![
            Message::text(Role::User, "What is 6*7?"),
            Message::new(
                Role::Assistant,
                vec![ContentPart::tool_call("call_1", "calculator", r#"{"expression":"6 * 7"}"#)],
            ),
            Message::new(
                Role::Tool,
                vec![ContentPart::tool_result("call_1", "calculator", r#"{"result":42}"#, false)],
            ),
            Message::text(Role::User, "Thanks"),
        ])
        .with_tools(vec![ai_core::ToolDefinition::new(
            "calculator",
            "Evaluates expressions",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}),
        )]);

        let body = build_messages_body(&request, "claude-sonnet-4").unwrap();
        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        // Tool result lands in the next user turn as a tool_result block.
        let thanks_turn = &body["messages"][2];
        assert_eq!(thanks_turn["role"], "user");
        assert_eq!(thanks_turn["content"][0]["type"], "tool_result");
        assert_eq!(thanks_turn["content"][0]["tool_use_id"], "call_1");
        assert_eq!(thanks_turn["content"][0]["content"]["result"], 42);
        assert_eq!(body["tools"][0]["name"], "calculator");
        assert_eq!(body["max_tokens"], 4096);
    }

    #[test]
    fn serializes_images_as_base64_sources() {
        let request = ChatRequest::new(vec![Message::new(
            Role::User,
            vec![
                ContentPart::text("what is this?"),
                ContentPart::Image {
                    image: ai_types::ImageSource::Base64 {
                        media_type: "image/png".into(),
                        data: "AAAA".into(),
                    },
                },
            ],
        )]);
        let body = build_messages_body(&request, "claude-sonnet-4").unwrap();
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn parses_non_streaming_response() {
        let json = json!({
            "id": "msg_1",
            "content": [
                {"type": "text", "text": "The answer is "},
                {"type": "tool_use", "id": "call_9", "name": "calculator", "input": {"expression": "6 * 7"}},
                {"type": "text", "text": "42."}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 12, "output_tokens": 8, "cache_read_input_tokens": 4}
        });
        let completion = parse_messages_response(
            &ProviderId::new("anthropic"),
            &ModelId::new("claude-sonnet-4"),
            &json,
        )
        .unwrap();
        assert_eq!(completion.text, "The answer is 42.");
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].name, "calculator");
        assert!(completion.tool_calls[0].arguments.contains("6 * 7"));
        assert_eq!(completion.finish_reason.as_deref(), Some("tool_use"));
        assert_eq!(completion.usage.cached_input_tokens, Some(4));
    }

    #[tokio::test]
    async fn maps_sse_events_to_unified_stream() {
        let chunks = vec![
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"calculator\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"expression\\\":\\\"6 * 7\\\"}\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
        ];
        let input = futures::stream::iter(
            chunks
                .into_iter()
                .map(|c| Ok::<bytes::Bytes, AiError>(bytes::Bytes::from(c))),
        );
        let events: Vec<StreamEvent> = map_anthropic_sse(sse_parse(input))
            .map(|e| e.unwrap())
            .collect()
            .await;

        assert!(matches!(events[0], StreamEvent::UsageUpdate { .. }));
        assert!(matches!(
            events[1],
            StreamEvent::ToolCallStarted { ref id, .. } if id == "call_1"
        ));
        assert!(matches!(events[2], StreamEvent::ToolCallDelta { .. }));
        // Finalization happens at message_delta.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ToolCallCompleted { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::Completed { .. }))
        );
    }
}
