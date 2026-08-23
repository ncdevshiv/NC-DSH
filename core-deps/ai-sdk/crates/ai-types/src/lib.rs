//! Core domain types for the AI SDK.
//!
//! These types are the language of the SDK: messages with typed content
//! parts, roles, token usage, and identifiers. They are serializable so they
//! can cross process/thread/provider boundaries and be persisted.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifier for a provider (e.g. `openai`, `anthropic`, `google`).
///
/// A lightweight newtype over `String` that guarantees non-empty values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderId(String);

impl ProviderId {
    /// Creates a [`ProviderId`]. If input is empty, defaults to `"unknown"`.
    /// Prefer [`ProviderId::try_new`] for fallible validation.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        if id.is_empty() {
            Self("unknown".to_string())
        } else {
            Self(id)
        }
    }

    /// Creates a [`ProviderId`], returning `None` for empty input.
    pub fn try_new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        (!id.is_empty()).then_some(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ProviderId> for String {
    fn from(id: ProviderId) -> Self {
        id.0
    }
}

/// Identifier for a model (e.g. `gpt-4o`, `claude-3-5-sonnet-20241022`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModelId(String);

impl ModelId {
    /// Creates a [`ModelId`]. If input is empty, defaults to `"unknown"`.
    /// Prefer [`ModelId::try_new`] for fallible validation.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        if id.is_empty() {
            Self("unknown".to_string())
        } else {
            Self(id)
        }
    }

    pub fn try_new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        (!id.is_empty()).then_some(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ModelId> for String {
    fn from(id: ModelId) -> Self {
        id.0
    }
}

/// The role of a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System instructions/context.
    System,
    /// End-user input.
    User,
    /// Model/assistant output.
    Assistant,
    /// Tool execution result.
    Tool,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        };
        f.write_str(s)
    }
}

/// Modalities a model or message can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
}

impl fmt::Display for Modality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
        };
        f.write_str(s)
    }
}

/// A reference to image data, either by URL or inline bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum ImageSource {
    /// Fetch the image from `url`.
    Url { url: String },
    /// Inline base64-encoded bytes.
    Base64 {
        /// MIME type, e.g. `image/png`.
        media_type: String,
        /// Base64-encoded payload.
        data: String,
    },
}

/// A reference to audio data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum AudioSource {
    /// Fetch the audio from `url`.
    Url { url: String },
    /// Inline base64-encoded bytes.
    Base64 {
        /// MIME type, e.g. `audio/wav`.
        media_type: String,
        /// Base64-encoded payload.
        data: String,
    },
}

/// A tool call the assistant requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned id used to correlate the result.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON-encoded arguments.
    pub arguments: String,
}

/// A tool execution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Correlates with [`ToolCall::id`].
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON-encoded result value.
    pub output: String,
    /// Whether the execution failed; `output` then carries the error.
    pub is_error: bool,
}

/// A typed part of a message's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text content.
    Text { text: String },
    /// Image input (vision).
    Image { image: ImageSource },
    /// Audio input.
    Audio { audio: AudioSource },
    /// A tool call requested by the assistant.
    ToolCall { call: ToolCall },
    /// The result of a tool call.
    ToolResult { result: ToolResult },
}

impl ContentPart {
    /// Convenience constructor for text parts.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Convenience constructor for image parts from a URL.
    pub fn image_url(url: impl Into<String>) -> Self {
        Self::Image {
            image: ImageSource::Url { url: url.into() },
        }
    }

    /// Convenience constructor for tool call parts.
    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self::ToolCall {
            call: ToolCall {
                id: id.into(),
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }

    /// Convenience constructor for tool result parts.
    pub fn tool_result(
        id: impl Into<String>,
        name: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            result: ToolResult {
                id: id.into(),
                name: name.into(),
                output: output.into(),
                is_error,
            },
        }
    }
}

/// A message in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    /// Ordered content parts; at least one part is required.
    pub parts: Vec<ContentPart>,
    /// Optional participant name (e.g. sub-agent id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn new(role: Role, parts: Vec<ContentPart>) -> Self {
        let parts = if parts.is_empty() {
            vec![ContentPart::text("")]
        } else {
            parts
        };
        Self {
            role,
            parts,
            name: None,
        }
    }

    /// Creates a simple text message.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self::new(role, vec![ContentPart::text(text)])
    }

    /// Concatenates all text parts into a single string (for debugging,
    /// logging, or text-only targets). Non-text parts are ignored.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Token usage for a model interaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens (including cached tokens when applicable).
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Reasoning tokens (when the model exposes them separately).
    pub reasoning_tokens: Option<u64>,
    /// Cached input tokens (provider-reported cache reads).
    pub cached_input_tokens: Option<u64>,
    /// Total tokens, when provided by the provider.
    pub total_tokens: Option<u64>,
}

impl Usage {
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            reasoning_tokens: None,
            cached_input_tokens: None,
            total_tokens: None,
        }
    }

    pub fn total(&self) -> u64 {
        self.total_tokens
            .unwrap_or(self.input_tokens + self.output_tokens)
    }
}

/// A complete (non-streaming) model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// The provider that produced this completion.
    pub provider: ProviderId,
    /// The model that produced this completion.
    pub model: ModelId,
    /// Text content of the response (concatenated text parts).
    pub text: String,
    /// Tool calls requested by the model, if any.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage.
    pub usage: Usage,
    /// Reasoning/thinking text, when the model exposes it separately
    /// (e.g. DeepSeek-style `reasoning_content`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Provider-specific fields surfaced for debugging.
    #[serde(default)]
    pub raw: serde_json::Value,
    /// Finish reason, when the provider reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// A structured event emitted by a streaming model call.
///
/// Providers convert their raw wire formats into these unified events
/// (see `ai-stream`); consumers never see provider-specific formats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A chunk of assistant text.
    TextDelta { delta: String },
    /// A chunk of reasoning/thinking text, when the model exposes it.
    ReasoningDelta { delta: String },
    /// A tool call began.
    ToolCallStarted { id: String, name: String },
    /// A chunk of JSON arguments for an in-flight tool call.
    ToolCallDelta { id: String, arguments_delta: String },
    /// A tool call finished with complete arguments.
    ToolCallCompleted { call: ToolCall },
    /// Updated token usage (usually emitted once, near the end).
    UsageUpdate { usage: Usage },
    /// A recoverable mid-stream error (does not abort the stream).
    Error { message: String },
    /// The stream finished.
    Completed { finish_reason: Option<String> },
}

impl StreamEvent {
    /// True for events that carry assistant-visible text.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::TextDelta { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_text_roundtrip() {
        let m = Message::text(Role::User, "hello");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.text_content(), "hello");
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn content_parts_serialize_with_tags() {
        let msg = Message::new(
            Role::Assistant,
            vec![
                ContentPart::text("let me check"),
                ContentPart::tool_call("call_1", "calculator", r#"{"expr":"2+2"}"#),
            ],
        );
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""), "{json}");
        assert!(json.contains("\"call_1\""), "{json}");
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.parts.len(), 2);
    }

    #[test]
    fn image_part_serializes_url_source() {
        let p = ContentPart::image_url("https://example.com/a.png");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"source\":\"url\""), "{json}");
    }

    #[test]
    fn ids_reject_empty() {
        assert!(ProviderId::try_new("").is_none());
        assert!(ModelId::try_new("").is_none());
        assert_eq!(ProviderId::new("openai").as_str(), "openai");
    }

    #[test]
    fn usage_total() {
        let u = Usage::new(10, 20);
        assert_eq!(u.total(), 30);
        let mut u2 = Usage::new(10, 20);
        u2.total_tokens = Some(35);
        assert_eq!(u2.total(), 35);
    }

    #[test]
    fn stream_event_roundtrip() {
        let events = vec![
            StreamEvent::TextDelta {
                delta: "hel".into(),
            },
            StreamEvent::ToolCallStarted {
                id: "call_1".into(),
                name: "calc".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_1".into(),
                arguments_delta: r#"{"e"#.into(),
            },
            StreamEvent::ToolCallCompleted {
                call: ToolCall {
                    id: "call_1".into(),
                    name: "calc".into(),
                    arguments: r#"{"expr":"2+2"}"#.into(),
                },
            },
            StreamEvent::UsageUpdate {
                usage: Usage::new(10, 5),
            },
            StreamEvent::Completed {
                finish_reason: Some("stop".into()),
            },
        ];
        for e in &events {
            let json = serde_json::to_string(e).unwrap();
            let back: StreamEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(e, &back, "{json}");
        }
        assert!(events[0].is_text());
        assert!(!events[1].is_text());
    }
}
