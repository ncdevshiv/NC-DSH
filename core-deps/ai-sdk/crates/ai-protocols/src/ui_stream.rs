//! Vercel AI SDK UI Data Stream Protocol framing (`0:`, `8:`, `9:`, `e:`, `d:`).
//!
//! Provides 100% wire-compatibility with Vercel AI SDK frontend hooks (`useChat`, `useCompletion`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use ai_errors::{AiError, SerializationError};
use ai_types::Usage;

/// A parsed part of a Vercel AI SDK UI Data Stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiDataStreamPart {
    TextDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        tool_call_id: String,
        result: Value,
    },
    Error {
        message: String,
    },
    Finish {
        finish_reason: String,
        usage: Option<Usage>,
    },
}

/// Helper for framing and parsing Vercel AI SDK Data Stream lines.
pub struct UiDataStreamFramer;

impl UiDataStreamFramer {
    /// Formats a text delta chunk (`0:"text"`).
    pub fn encode_text_delta(text: &str) -> String {
        format!("0:{}\n", json!(text))
    }

    /// Formats a tool call chunk (`8:{"id":...,"name":...,"arguments":...}`).
    pub fn encode_tool_call(id: &str, name: &str, args: &Value) -> String {
        let payload = json!({
            "id": id,
            "name": name,
            "arguments": args
        });
        format!("8:{payload}\n")
    }

    /// Formats a tool result chunk (`9:{"toolCallId":...,"result":...}`).
    pub fn encode_tool_result(tool_call_id: &str, result: &Value) -> String {
        let payload = json!({
            "toolCallId": tool_call_id,
            "result": result
        });
        format!("9:{payload}\n")
    }

    /// Formats an error chunk (`e:"error"`).
    pub fn encode_error(message: &str) -> String {
        format!("e:{}\n", json!(message))
    }

    /// Formats a stream finish chunk (`d:{"finishReason":...,"usage":...}`).
    pub fn encode_finish(finish_reason: &str, usage: Option<&Usage>) -> String {
        let mut payload = json!({ "finishReason": finish_reason });
        if let Some(u) = usage {
            payload["usage"] = json!({
                "promptTokens": u.input_tokens,
                "completionTokens": u.output_tokens
            });
        }
        format!("d:{payload}\n")
    }

    /// Parses a single line from a Vercel AI SDK Data Stream into a [`UiDataStreamPart`].
    pub fn parse_line(line: &str) -> Result<UiDataStreamPart, AiError> {
        let line = line.trim_end_matches('\r').trim_end_matches('\n');
        if line.len() < 2 || !line.as_bytes()[1] == b':' {
            return Err(AiError::Serialization(SerializationError::new(
                "invalid UI stream line format",
            )));
        }

        let prefix = &line[..1];
        let payload = &line[2..];

        match prefix {
            "0" => {
                let text: String = serde_json::from_str(payload)
                    .map_err(|e| SerializationError::new(e.to_string()))?;
                Ok(UiDataStreamPart::TextDelta { text })
            }
            "8" => {
                let val: Value = serde_json::from_str(payload)
                    .map_err(|e| SerializationError::new(e.to_string()))?;
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = val.get("arguments").cloned().unwrap_or(Value::Null);
                Ok(UiDataStreamPart::ToolCall {
                    id,
                    name,
                    arguments,
                })
            }
            "9" => {
                let val: Value = serde_json::from_str(payload)
                    .map_err(|e| SerializationError::new(e.to_string()))?;
                let tool_call_id = val
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let result = val.get("result").cloned().unwrap_or(Value::Null);
                Ok(UiDataStreamPart::ToolResult {
                    tool_call_id,
                    result,
                })
            }
            "e" => {
                let message: String = serde_json::from_str(payload)
                    .map_err(|e| SerializationError::new(e.to_string()))?;
                Ok(UiDataStreamPart::Error { message })
            }
            "d" => {
                let val: Value = serde_json::from_str(payload)
                    .map_err(|e| SerializationError::new(e.to_string()))?;
                let finish_reason = val
                    .get("finishReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stop")
                    .to_string();
                Ok(UiDataStreamPart::Finish {
                    finish_reason,
                    usage: None,
                })
            }
            _ => Err(AiError::Serialization(SerializationError::new(format!(
                "unknown UI stream prefix `{prefix}`"
            )))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_stream_text_delta_encodes_and_parses() {
        let line = UiDataStreamFramer::encode_text_delta("Hello world");
        assert_eq!(line, "0:\"Hello world\"\n");
        let part = UiDataStreamFramer::parse_line(&line).unwrap();
        assert_eq!(
            part,
            UiDataStreamPart::TextDelta {
                text: "Hello world".to_string()
            }
        );
    }

    #[test]
    fn ui_stream_tool_call_encodes_and_parses() {
        let args = json!({"x": 6, "y": 7});
        let line = UiDataStreamFramer::encode_tool_call("call_1", "multiply", &args);
        assert!(line.starts_with("8:"));
        let part = UiDataStreamFramer::parse_line(&line).unwrap();
        if let UiDataStreamPart::ToolCall {
            id,
            name,
            arguments,
        } = part
        {
            assert_eq!(id, "call_1");
            assert_eq!(name, "multiply");
            assert_eq!(arguments["x"], 6);
        } else {
            panic!("expected ToolCall");
        }
    }
}
