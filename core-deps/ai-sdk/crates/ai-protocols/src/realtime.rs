//! Realtime WebSocket / SSE protocol event framing and session models.
//!
//! Provides bidirectional event serialization and parsing for low-latency
//! multimodal Realtime sessions (OpenAI Realtime / Gemini Multimodal Live compatible).
//!
//! Server-event parsing is tolerant by design: recognized event types decode
//! into their typed variants, anything unrecognized is preserved verbatim in
//! [`RealtimeServerEvent::Other`] so a session never stalls on a newer
//! provider vocabulary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use ai_errors::{AiError, SerializationError};

/// Realtime session configuration options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RealtimeSessionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_response_output_tokens: Option<u64>,
}

/// Events sent from client to server in a Realtime session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeClientEvent {
    /// Update session configuration.
    #[serde(rename = "session.update")]
    SessionUpdate {
        event_id: String,
        session: RealtimeSessionConfig,
    },
    /// Append base64 audio payload to the input buffer.
    #[serde(rename = "input_audio_buffer.append")]
    InputAudioBufferAppend { event_id: String, audio: String },
    /// Commit the input audio buffer for processing.
    #[serde(rename = "input_audio_buffer.commit")]
    InputAudioBufferCommit { event_id: String },
    /// Clear the input audio buffer.
    #[serde(rename = "input_audio_buffer.clear")]
    InputAudioBufferClear { event_id: String },
    /// Create a new conversation item.
    #[serde(rename = "conversation.item.create")]
    ItemCreate { event_id: String, item: Value },
    /// Trigger response generation.
    #[serde(rename = "response.create")]
    ResponseCreate {
        event_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response: Option<Value>,
    },
    /// Cancel in-flight response.
    #[serde(rename = "response.cancel")]
    ResponseCancel { event_id: String },
}

/// Events emitted by server to client in a Realtime session.
///
/// Wire tags (`"type"` values, see [`Self::type_name`]) are stable and
/// OpenAI-Realtime compatible. Unknown event types deserialize into
/// [`RealtimeServerEvent::Other`] with their JSON preserved verbatim instead
/// of failing the stream; serialization of `Other` re-emits that JSON as-is.
#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeServerEvent {
    SessionCreated {
        event_id: String,
        session: Value,
    },
    SessionUpdated {
        event_id: String,
        session: Value,
    },
    ItemCreated {
        event_id: String,
        item: Value,
    },
    /// Server-side VAD detected speech start in the user's input audio.
    InputAudioBufferSpeechStarted {
        event_id: String,
        /// Milliseconds from the start of all input audio to speech onset.
        audio_start_ms: u64,
        item_id: String,
    },
    /// Server-side VAD detected speech end in the user's input audio.
    InputAudioBufferSpeechStopped {
        event_id: String,
        /// Milliseconds from the start of all input audio to speech offset.
        audio_end_ms: u64,
        item_id: String,
    },
    ResponseTextDelta {
        event_id: String,
        response_id: String,
        output_index: u32,
        delta: String,
    },
    /// Incremental base64-encoded TTS audio chunk for playback.
    ResponseAudioDelta {
        event_id: String,
        response_id: String,
        output_index: u32,
        delta: String,
    },
    /// The in-flight response was cancelled -- the canonical interruption
    /// signal driving barge-in cancellation.
    ResponseCancelled {
        event_id: String,
        response_id: String,
    },
    ResponseDone {
        event_id: String,
        response: Value,
    },
    Error {
        event_id: String,
        error: Value,
    },
    /// An unrecognized server event, preserved verbatim (including its
    /// original `"type"` field inside `raw`).
    Other {
        event_id: Option<String>,
        raw: Value,
    },
}

impl RealtimeServerEvent {
    /// The provider event id when present.
    pub fn event_id(&self) -> Option<&str> {
        match self {
            Self::SessionCreated { event_id, .. }
            | Self::SessionUpdated { event_id, .. }
            | Self::ItemCreated { event_id, .. }
            | Self::InputAudioBufferSpeechStarted { event_id, .. }
            | Self::InputAudioBufferSpeechStopped { event_id, .. }
            | Self::ResponseTextDelta { event_id, .. }
            | Self::ResponseAudioDelta { event_id, .. }
            | Self::ResponseCancelled { event_id, .. }
            | Self::ResponseDone { event_id, .. }
            | Self::Error { event_id, .. } => Some(event_id),
            Self::Other { event_id, .. } => event_id.as_deref(),
        }
    }

    /// True when this event signals that playback of the current response
    /// should stop (barge-in trigger on the receive path).
    pub fn is_interruption(&self) -> bool {
        matches!(self, Self::ResponseCancelled { .. })
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "session.created",
            Self::SessionUpdated { .. } => "session.updated",
            Self::ItemCreated { .. } => "conversation.item.created",
            Self::InputAudioBufferSpeechStarted { .. } => "input_audio_buffer.speech_started",
            Self::InputAudioBufferSpeechStopped { .. } => "input_audio_buffer.speech_stopped",
            Self::ResponseTextDelta { .. } => "response.text.delta",
            Self::ResponseAudioDelta { .. } => "response.audio.delta",
            Self::ResponseCancelled { .. } => "response.cancelled",
            Self::ResponseDone { .. } => "response.done",
            Self::Error { .. } => "error",
            Self::Other { .. } => "",
        }
    }

    fn str_field(v: &Value, key: &str) -> String {
        v.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn u64_field(v: &Value, key: &str) -> u64 {
        v.get(key).and_then(Value::as_u64).unwrap_or(0)
    }

    fn u32_field(v: &Value, key: &str) -> u32 {
        v.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
    }
}

impl Serialize for RealtimeServerEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Self::Other { raw, .. } = self {
            return raw.serialize(serializer);
        }
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), Value::String(self.type_name().into()));
        if let Some(event_id) = self.event_id() {
            obj.insert("event_id".into(), Value::String(event_id.to_string()));
        }
        match self {
            Self::SessionCreated { session, .. } | Self::SessionUpdated { session, .. } => {
                obj.insert("session".into(), session.clone());
            }
            Self::ItemCreated { item, .. } => {
                obj.insert("item".into(), item.clone());
            }
            Self::InputAudioBufferSpeechStarted {
                audio_start_ms,
                item_id,
                ..
            } => {
                obj.insert("audio_start_ms".into(), Value::from(*audio_start_ms));
                obj.insert("item_id".into(), Value::String(item_id.clone()));
            }
            Self::InputAudioBufferSpeechStopped {
                audio_end_ms,
                item_id,
                ..
            } => {
                obj.insert("audio_end_ms".into(), Value::from(*audio_end_ms));
                obj.insert("item_id".into(), Value::String(item_id.clone()));
            }
            Self::ResponseTextDelta {
                response_id,
                output_index,
                delta,
                ..
            }
            | Self::ResponseAudioDelta {
                response_id,
                output_index,
                delta,
                ..
            } => {
                obj.insert("response_id".into(), Value::String(response_id.clone()));
                obj.insert("output_index".into(), Value::from(*output_index));
                obj.insert("delta".into(), Value::String(delta.clone()));
            }
            Self::ResponseCancelled { response_id, .. } => {
                obj.insert("response_id".into(), Value::String(response_id.clone()));
            }
            Self::ResponseDone { response, .. } => {
                obj.insert("response".into(), response.clone());
            }
            Self::Error { error, .. } => {
                obj.insert("error".into(), error.clone());
            }
            Self::Other { .. } => unreachable!("handled above"),
        }
        Value::Object(obj).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RealtimeServerEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        Ok(Self::from_value(v))
    }
}

impl RealtimeServerEvent {
    /// Classifies a decoded JSON object into a typed server event.
    ///
    /// Malformed payloads for a *recognized* type still fall back to
    /// [`RealtimeServerEvent::Other`] rather than erroring: the stream stays
    /// alive and callers can inspect `raw`.
    pub fn from_value(v: Value) -> Self {
        let ty = v.get("type").and_then(Value::as_str).unwrap_or_default();
        let event_id = Self::str_field(&v, "event_id");
        match ty {
            "session.created" => Self::SessionCreated {
                event_id,
                session: v.get("session").cloned().unwrap_or(Value::Null),
            },
            "session.updated" => Self::SessionUpdated {
                event_id,
                session: v.get("session").cloned().unwrap_or(Value::Null),
            },
            "conversation.item.created" => Self::ItemCreated {
                event_id,
                item: v.get("item").cloned().unwrap_or(Value::Null),
            },
            "input_audio_buffer.speech_started" => Self::InputAudioBufferSpeechStarted {
                event_id,
                audio_start_ms: Self::u64_field(&v, "audio_start_ms"),
                item_id: Self::str_field(&v, "item_id"),
            },
            "input_audio_buffer.speech_stopped" => Self::InputAudioBufferSpeechStopped {
                event_id,
                audio_end_ms: Self::u64_field(&v, "audio_end_ms"),
                item_id: Self::str_field(&v, "item_id"),
            },
            "response.text.delta" => Self::ResponseTextDelta {
                event_id,
                response_id: Self::str_field(&v, "response_id"),
                output_index: Self::u32_field(&v, "output_index"),
                delta: Self::str_field(&v, "delta"),
            },
            "response.audio.delta" => Self::ResponseAudioDelta {
                event_id,
                response_id: Self::str_field(&v, "response_id"),
                output_index: Self::u32_field(&v, "output_index"),
                delta: Self::str_field(&v, "delta"),
            },
            "response.cancelled" => Self::ResponseCancelled {
                event_id,
                response_id: Self::str_field(&v, "response_id"),
            },
            "response.done" => Self::ResponseDone {
                event_id,
                response: v.get("response").cloned().unwrap_or(Value::Null),
            },
            "error" => Self::Error {
                event_id,
                error: v.get("error").cloned().unwrap_or(Value::Null),
            },
            _ => Self::Other {
                event_id: (!event_id.is_empty()).then_some(event_id),
                raw: v,
            },
        }
    }
}

/// Helper for parsing and serializing Realtime protocol frames.
pub struct RealtimeEventFramer;

impl RealtimeEventFramer {
    /// Parses a raw JSON frame into a [`RealtimeServerEvent`].
    pub fn parse_server_event(json_payload: &str) -> Result<RealtimeServerEvent, AiError> {
        serde_json::from_str(json_payload).map_err(|e| {
            AiError::Serialization(SerializationError::new(format!(
                "failed to parse Realtime server event: {e}"
            )))
        })
    }

    /// Serializes a [`RealtimeClientEvent`] into a JSON text frame.
    pub fn serialize_client_event(event: &RealtimeClientEvent) -> Result<String, AiError> {
        serde_json::to_string(event).map_err(|e| {
            AiError::Serialization(SerializationError::new(format!(
                "failed to serialize Realtime client event: {e}"
            )))
        })
    }

    /// Serializes a [`RealtimeServerEvent`] back into a JSON frame
    /// (round-trip / echo-server testing).
    pub fn serialize_server_event(event: &RealtimeServerEvent) -> Result<String, AiError> {
        serde_json::to_string(event).map_err(|e| {
            AiError::Serialization(SerializationError::new(format!(
                "failed to serialize Realtime server event: {e}"
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_event_roundtrips() {
        let event = RealtimeClientEvent::InputAudioBufferAppend {
            event_id: "evt_123".into(),
            audio: "base64audiobytes==".into(),
        };
        let json = RealtimeEventFramer::serialize_client_event(&event).unwrap();
        assert!(json.contains("input_audio_buffer.append"));
        assert!(json.contains("base64audiobytes=="));
    }

    #[test]
    fn server_text_delta_parses() {
        let raw = r#"{
            "type": "response.text.delta",
            "event_id": "evt_456",
            "response_id": "resp_001",
            "output_index": 0,
            "delta": "Hello world"
        }"#;
        let event = RealtimeEventFramer::parse_server_event(raw).unwrap();
        if let RealtimeServerEvent::ResponseTextDelta { delta, .. } = event {
            assert_eq!(delta, "Hello world");
        } else {
            panic!("expected ResponseTextDelta");
        }
    }

    #[test]
    fn unknown_event_type_is_preserved_not_dropped() {
        let raw = r#"{"type":"brand.new.event","event_id":"evt_x","foo":{"bar":1}}"#;
        let event = RealtimeEventFramer::parse_server_event(raw).unwrap();
        match &event {
            RealtimeServerEvent::Other { event_id, raw } => {
                assert_eq!(event_id.as_deref(), Some("evt_x"));
                assert_eq!(raw["type"], "brand.new.event");
                assert_eq!(raw["foo"]["bar"], 1);
            }
            other => panic!("expected Other, got {other:?}"),
        }
        // Round-trips verbatim through serialization.
        let json = RealtimeEventFramer::serialize_server_event(&event).unwrap();
        assert!(json.contains("brand.new.event"));
    }

    #[test]
    fn interruption_flag_and_speech_events() {
        assert!(
            RealtimeEventFramer::parse_server_event(
                r#"{"type":"response.cancelled","event_id":"e1","response_id":"r1"}"#
            )
            .unwrap()
            .is_interruption()
        );
        let started = RealtimeEventFramer::parse_server_event(
            r#"{"type":"input_audio_buffer.speech_started","event_id":"e2","audio_start_ms":120,"item_id":"item_1"}"#,
        )
        .unwrap();
        assert_eq!(started.event_id(), Some("e2"));
        match started {
            RealtimeServerEvent::InputAudioBufferSpeechStarted {
                audio_start_ms,
                item_id,
                ..
            } => {
                assert_eq!(audio_start_ms, 120);
                assert_eq!(item_id, "item_1");
            }
            other => panic!("expected SpeechStarted, got {other:?}"),
        }
    }

    #[test]
    fn server_event_roundtrips_through_wire_format() {
        for event in [
            RealtimeServerEvent::ResponseAudioDelta {
                event_id: "evt_a".into(),
                response_id: "resp_a".into(),
                output_index: 2,
                delta: "QUJD".into(),
            },
            RealtimeServerEvent::InputAudioBufferSpeechStopped {
                event_id: "evt_b".into(),
                audio_end_ms: 4_512,
                item_id: "item_b".into(),
            },
            RealtimeServerEvent::ResponseCancelled {
                event_id: "evt_c".into(),
                response_id: "resp_c".into(),
            },
            RealtimeServerEvent::Error {
                event_id: "evt_e".into(),
                error: serde_json::json!({"code": "rate_limited"}),
            },
        ] {
            let json = RealtimeEventFramer::serialize_server_event(&event).unwrap();
            assert!(json.contains("\"type\":"));
            let parsed = RealtimeEventFramer::parse_server_event(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }
}
