//! Google Gemini native adapter: real `generateContent` wire format with
//! `x-goog-api-key` auth, function declarations, inline data images, and
//! SSE streaming (`alt=sse`).
//!
//! Requires `GOOGLE_API_KEY`. Untested live without credentials
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

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};

/// Configuration for the Gemini provider.
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout: Duration,
}

impl GeminiConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn from_provider_config(cfg: &ai_config::ProviderConfig) -> Result<Self, AiError> {
        let api_key = cfg.require_api_key("google")?.to_string();
        let base_url = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
        Ok(Self {
            api_key,
            base_url,
            timeout: Duration::from_secs(60),
        })
    }
}

/// The Gemini provider (real HTTP adapter).
pub struct GeminiProvider {
    config: GeminiConfig,
    http: HttpClient,
}

impl std::fmt::Debug for GeminiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("base_url", &self.config.base_url)
            .field("api_key", &"***redacted***")
            .finish()
    }
}

impl GeminiProvider {
    pub fn new(config: GeminiConfig) -> Result<Self, AiError> {
        Ok(Self {
            config,
            http: HttpClient::shared(),
        })
    }

    fn url(&self, model: &str, stream: bool) -> String {
        let method = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let mut url = format!(
            "{}/models/{}:{}",
            self.config.base_url.trim_end_matches('/'),
            model,
            method
        );
        if stream {
            url.push_str("?alt=sse");
        }
        url
    }
}

/// Serializes a unified request into the Gemini `generateContent` format.
fn build_generate_body(request: &ChatRequest) -> Result<Value, AiError> {
    let mut contents: Vec<Value> = Vec::new();
    let mut pending_tool_results: Vec<Value> = Vec::new();

    for message in &request.messages {
        match message.role {
            Role::System => {
                // Gemini has no system role; prepend as a user part in the
                // first turn (documented convention).
                if contents.is_empty() {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{"text": message.text_content()}]
                    }));
                } else if let Some(parts) = contents[0]["parts"].as_array_mut() {
                    parts.push(json!({"text": message.text_content()}));
                }
            }
            Role::User => {
                let mut parts: Vec<Value> = std::mem::take(&mut pending_tool_results);
                parts.extend(serialize_parts(message)?);
                contents.push(json!({"role": "user", "parts": parts}));
            }
            Role::Assistant => {
                let mut parts: Vec<Value> = Vec::new();
                let text = message.text_content();
                if !text.is_empty() {
                    parts.push(json!({"text": text}));
                }
                for part in &message.parts {
                    if let ContentPart::ToolCall { call } = part {
                        parts.push(json!({
                            "functionCall": {
                                "name": call.name,
                                "args": parse_json_args(&call.arguments),
                            }
                        }));
                    }
                }
                contents.push(json!({"role": "model", "parts": parts}));
            }
            Role::Tool => {
                for part in &message.parts {
                    if let ContentPart::ToolResult { result } = part {
                        pending_tool_results.push(json!({
                            "functionResponse": {
                                "name": result.name,
                                "response": {
                                    "result": parse_json_args(&result.output),
                                    "is_error": result.is_error,
                                },
                            }
                        }));
                    }
                }
            }
        }
    }

    if !pending_tool_results.is_empty() {
        contents.push(json!({"role": "user", "parts": pending_tool_results}));
    }

    let mut body = json!({
        "contents": contents,
    });
    if !request.tools.is_empty() {
        let declarations: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();
        body["tools"] = json!([{"functionDeclarations": declarations}]);
    }
    let mut generation_config = serde_json::Map::new();
    if let Some(temperature) = request.temperature {
        generation_config.insert("temperature".into(), json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        generation_config.insert("maxOutputTokens".into(), json!(max_tokens));
    }
    if matches!(
        request.response_format,
        ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. }
    ) {
        generation_config.insert("responseMimeType".into(), json!("application/json"));
    }
    body["generationConfig"] = Value::Object(generation_config);
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

fn serialize_parts(message: &Message) -> Result<Vec<Value>, AiError> {
    let mut parts: Vec<Value> = Vec::new();
    let text = message.text_content();
    if !text.is_empty() {
        parts.push(json!({"text": text}));
    }
    for part in &message.parts {
        if let ContentPart::Image { image } = part {
            let inline = match image {
                ai_types::ImageSource::Url { url } => {
                    // Gemini requires inline data; URL images need
                    // fetching, which we do not do silently — the
                    // provider must pass base64 data for remote URLs.
                    return Err(AiError::Serialization(SerializationError::new(format!(
                        "Gemini requires inline image data; URL images are not supported: {url}"
                    ))));
                }
                ai_types::ImageSource::Base64 { media_type, data } => {
                    json!({
                        "mimeType": media_type,
                        "data": data,
                    })
                }
            };
            parts.push(json!({"inlineData": inline}));
        }
    }
    Ok(parts)
}

/// Parses a non-streaming `generateContent` response.
fn parse_generate_response(
    provider: &ProviderId,
    model: &ModelId,
    json: &Value,
) -> Result<Completion, AiError> {
    let candidate = json
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .ok_or_else(|| {
            AiError::Serialization(SerializationError::new(
                "Gemini response missing `candidates`",
            ))
        })?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut finish_reason = candidate
        .get("finishReason")
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    if let Some(parts) = candidate
        .pointer("/content/parts")
        .and_then(|p| p.as_array())
    {
        for part in parts {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
            if let Some(call) = part.get("functionCall") {
                let name = call.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = call.get("args").cloned().unwrap_or(Value::Null);
                tool_calls.push(ToolCall {
                    id: format!("gemini-{name}"),
                    name: name.to_string(),
                    arguments: serde_json::to_string(&args).unwrap_or_else(|_| "{}".into()),
                });
            }
        }
    }
    if tool_calls.is_empty() && finish_reason.as_deref() == Some("STOP") {
        finish_reason = Some("stop".to_string());
    }

    let usage = json
        .get("usageMetadata")
        .map(|u| Usage {
            input_tokens: u
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: u
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            reasoning_tokens: None,
            cached_input_tokens: None,
            total_tokens: u.get("totalTokenCount").and_then(|v| v.as_u64()),
        })
        .unwrap_or_default();

    Ok(Completion {
        provider: provider.clone(),
        model: model.clone(),
        text,
        tool_calls,
        usage,
        reasoning: None,
        raw: json.clone(),
        finish_reason,
    })
}

/// Converts Gemini SSE chunks (`data: {...}`) into unified events.
fn map_gemini_sse(sse: ai_stream::SseStream) -> EventStream {
    let stream = sse.flat_map(move |sse_result| {
        let mut out: Vec<Result<StreamEvent, AiError>> = Vec::new();
        match sse_result {
            Err(e) => out.push(Err(e)),
            Ok(event) => {
                if event.data == "[DONE]" {
                    return futures::stream::iter(out);
                }
                let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
                    return futures::stream::iter(out); // non-JSON lines skipped
                };
                if let Some(candidates) = payload.get("candidates").and_then(|c| c.as_array()) {
                    if let Some(candidate) = candidates.first() {
                        if let Some(parts) = candidate
                            .pointer("/content/parts")
                            .and_then(|p| p.as_array())
                        {
                            for part in parts {
                                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                    if !t.is_empty() {
                                        out.push(Ok(StreamEvent::TextDelta {
                                            delta: t.to_string(),
                                        }));
                                    }
                                }
                                if let Some(call) = part.get("functionCall") {
                                    let name =
                                        call.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                    let args = call.get("args").cloned().unwrap_or(Value::Null);
                                    let id = format!("gemini-{name}");
                                    out.push(Ok(StreamEvent::ToolCallStarted {
                                        id: id.clone(),
                                        name: name.to_string(),
                                    }));
                                    out.push(Ok(StreamEvent::ToolCallDelta {
                                        id,
                                        arguments_delta: serde_json::to_string(&args)
                                            .unwrap_or_else(|_| "{}".into()),
                                    }));
                                }
                            }
                        }
                        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str())
                        {
                            out.push(Ok(StreamEvent::Completed {
                                finish_reason: Some(reason.to_string()),
                            }));
                        }
                    }
                }
                if let Some(usage) = payload.get("usageMetadata") {
                    out.push(Ok(StreamEvent::UsageUpdate {
                        usage: Usage {
                            input_tokens: usage
                                .get("promptTokenCount")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0),
                            output_tokens: usage
                                .get("candidatesTokenCount")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0),
                            reasoning_tokens: None,
                            cached_input_tokens: None,
                            total_tokens: usage.get("totalTokenCount").and_then(|v| v.as_u64()),
                        },
                    }));
                }
            }
        }
        futures::stream::iter(out)
    });
    Box::pin(stream)
}

#[async_trait]
impl Provider for GeminiProvider {
    fn id(&self) -> &str {
        "google"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError> {
        let response = tokio::time::timeout(
            self.config.timeout,
            self.http.execute(
                self.http
                    .get(format!(
                        "{}/models",
                        self.config.base_url.trim_end_matches('/')
                    ))
                    .header("x-goog-api-key", &self.config.api_key),
            ),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "google.models",
                self.config.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("google.models", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("google.models", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("google", status, retry_after, &bytes).await);
        }
        let json: Value = parse_json("google.models", &bytes)?;
        let models = json
            .get("models")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(models
            .into_iter()
            .filter_map(|m| {
                let name = m.get("name").and_then(|n| n.as_str())?;
                let id = name.rsplit('/').next().unwrap_or(name);
                let supported = m
                    .get("supportedGenerationMethods")
                    .and_then(|s| s.as_array())
                    .cloned()
                    .unwrap_or_default();
                let supports_streaming = supported
                    .iter()
                    .any(|s| s.as_str() == Some("streamGenerateContent"));
                let supports_tools = supported
                    .iter()
                    .any(|s| s.as_str() == Some("generateContent"));
                Some(
                    ModelInfo::new(
                        ProviderId::new("google"),
                        ModelId::new(id),
                        1_000_000,
                        8_192,
                    )
                    .with_name(id)
                    .with_capabilities(ModelCapabilities {
                        input_modalities: vec![ai_types::Modality::Text, ai_types::Modality::Image],
                        output_modalities: vec![ai_types::Modality::Text],
                        supports_streaming,
                        supports_tools,
                        supports_structured_output: true,
                        supports_embeddings: true,
                        supports_vision: true,
                        supports_fine_tuning: false,
                    }),
                )
            })
            .collect())
    }

    fn model(&self, model_id: &str) -> Result<Arc<dyn Model>, AiError> {
        let info = ModelInfo::new(
            ProviderId::new("google"),
            ModelId::new(model_id),
            1_000_000,
            8_192,
        )
        .with_name(model_id)
        .with_capabilities(ModelCapabilities {
            input_modalities: vec![ai_types::Modality::Text, ai_types::Modality::Image],
            output_modalities: vec![ai_types::Modality::Text],
            supports_streaming: true,
            supports_tools: true,
            supports_structured_output: true,
            supports_embeddings: true,
            supports_vision: true,
            supports_fine_tuning: false,
        });
        Ok(Arc::new(GeminiModel {
            provider: Arc::new(self.clone_for_model()),
            info,
        }))
    }
}

impl GeminiProvider {
    fn clone_for_model(&self) -> Self {
        Self {
            config: self.config.clone(),
            http: self.http.clone(),
        }
    }
}

/// A model behind the Gemini provider.
pub struct GeminiModel {
    provider: Arc<GeminiProvider>,
    info: ModelInfo,
}

#[async_trait]
impl Model for GeminiModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let body = build_generate_body(&request)?;
        let response = tokio::time::timeout(
            self.provider.config.timeout,
            self.provider.http.execute(
                self.provider
                    .http
                    .post(self.provider.url(self.info.id.as_str(), false))
                    .header("x-goog-api-key", &self.provider.config.api_key)
                    .json(&body),
            ),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "google.generate",
                self.provider.config.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("google.generate", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("google.generate", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("google", status, retry_after, &bytes).await);
        }
        let json: Value = parse_json("google.generate", &bytes)?;
        parse_generate_response(&self.info.provider, &self.info.id, &json)
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, AiError> {
        let body = build_generate_body(&request)?;
        let response = tokio::time::timeout(
            self.provider.config.timeout,
            self.provider.http.execute(
                self.provider
                    .http
                    .post(self.provider.url(self.info.id.as_str(), true))
                    .header("x-goog-api-key", &self.provider.config.api_key)
                    .json(&body),
            ),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "google.stream",
                self.provider.config.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("google.stream", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| map_reqwest_error("google.stream", e))?
                .to_vec();
            return Err(map_response_error("google", status, retry_after, &bytes).await);
        }
        let operation = "google.stream".to_string();
        let byte_stream = response
            .bytes_stream()
            .map(move |item| item.map_err(|e| map_reqwest_error(&operation, e)));
        Ok(map_gemini_sse(ai_stream::sse_parse(byte_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_messages_and_function_declarations() {
        let request = ChatRequest::new(vec![
            Message::text(Role::System, "You are helpful."),
            Message::text(Role::User, "What is 6*7?"),
            Message::new(
                Role::Assistant,
                vec![ContentPart::tool_call("gemini-calculator", "calculator", r#"{"expression":"6 * 7"}"#)],
            ),
            Message::new(
                Role::Tool,
                vec![ContentPart::tool_result("gemini-calculator", "calculator", r#"{"result":42}"#, false)],
            ),
        ])
        .with_tools(vec![ai_core::ToolDefinition::new(
            "calculator",
            "Evaluates expressions",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}),
        )])
        .with_temperature(0.5)
        .with_max_tokens(256);

        let body = build_generate_body(&request).unwrap();
        assert_eq!(body["contents"][0]["role"], "user");
        assert!(
            body["contents"][0]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .contains("You are helpful")
        );
        // contents[0] carries the system text; [1] the user question;
        // [2] the assistant tool call; [3] the tool response.
        assert_eq!(
            body["contents"][2]["parts"][0]["functionCall"]["name"],
            "calculator"
        );
        assert_eq!(
            body["contents"][3]["parts"][0]["functionResponse"]["name"],
            "calculator"
        );
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            "calculator"
        );
        assert_eq!(body["generationConfig"]["temperature"], 0.5);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 256);
    }

    #[test]
    fn json_format_sets_response_mime_type() {
        let request = ChatRequest::new(vec![Message::text(Role::User, "hi")])
            .with_response_format(ResponseFormat::JsonObject);
        let body = build_generate_body(&request).unwrap();
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
    }

    #[test]
    fn parses_generate_response_with_tool_call() {
        let json = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "Let me compute."},
                        {"functionCall": {"name": "calculator", "args": {"expression": "6 * 7"}}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 7, "totalTokenCount": 16}
        });
        let completion = parse_generate_response(
            &ProviderId::new("google"),
            &ModelId::new("gemini-1.5-pro"),
            &json,
        )
        .unwrap();
        assert_eq!(completion.text, "Let me compute.");
        assert_eq!(completion.tool_calls.len(), 1);
        assert!(completion.tool_calls[0].arguments.contains("6 * 7"));
        assert_eq!(completion.finish_reason.as_deref(), Some("STOP"));
        assert_eq!(completion.usage.total(), 16);
    }

    #[tokio::test]
    async fn maps_streaming_chunks() {
        let chunks = vec![
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":2,\"totalTokenCount\":5}}\n\n",
            "data: [DONE]\n\n",
        ];
        let input = futures::stream::iter(
            chunks
                .into_iter()
                .map(|c| Ok::<bytes::Bytes, AiError>(bytes::Bytes::from(c))),
        );
        let events: Vec<StreamEvent> = map_gemini_sse(ai_stream::sse_parse(input))
            .map(|e| e.unwrap())
            .collect()
            .await;
        assert!(matches!(events[0], StreamEvent::TextDelta { ref delta } if delta == "Hel"));
        assert!(matches!(events[1], StreamEvent::TextDelta { ref delta } if delta == "lo"));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::Completed { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::UsageUpdate { .. }))
        );
    }

    #[test]
    fn url_images_are_rejected_explicitly() {
        let request = ChatRequest::new(vec![Message::new(
            Role::User,
            vec![ContentPart::image_url("https://example.com/x.png")],
        )]);
        let err = build_generate_body(&request).unwrap_err();
        assert!(err.to_string().contains("inline"), "{err}");
    }
}
