use serde::Deserialize;
use std::str::FromStr;

pub(super) use chromiumoxide_cdp::cdp::browser_protocol::fetch::AuthChallengeResponseResponse;
pub(super) use chromiumoxide_cdp::cdp::browser_protocol::fetch::{
    ContinueRequestParams, ContinueResponseParams, ContinueWithAuthParams, HeaderEntry,
};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct EnableParams {
    #[serde(default)]
    pub(super) patterns: Vec<RequestPattern>,
    #[serde(default)]
    pub(super) handle_auth_requests: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RequestPattern {
    #[serde(default = "default_url_pattern")]
    pub(super) url_pattern: String,
    #[serde(default)]
    pub(super) resource_type: Option<String>,
    #[serde(default = "default_request_stage")]
    pub(super) request_stage: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RequestIdParam {
    pub(super) request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FailRequestParams {
    pub(super) request_id: String,
    #[serde(default)]
    pub(super) error_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FulfillRequestParams {
    pub(super) request_id: String,
    #[serde(default)]
    pub(super) response_code: Option<u16>,
    #[serde(default)]
    pub(super) response_headers: Option<Vec<HeaderEntry>>,
    #[serde(default)]
    pub(super) binary_response_headers: Option<String>,
    #[serde(default)]
    pub(super) body: Option<String>,
    #[serde(default)]
    pub(super) response_phrase: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DispatchWebSocketMessageParams {
    pub(super) request_id: String,
    #[serde(default = "default_websocket_opcode")]
    pub(super) opcode: String,
    pub(super) data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
pub(super) enum WebSocketMessageOpcode {
    #[strum(serialize = "Text", serialize = "text")]
    Text,
    #[strum(serialize = "Binary", serialize = "binary")]
    Binary,
}

impl WebSocketMessageOpcode {
    pub(super) fn parse(value: &str) -> Option<Self> {
        Self::from_str(value).ok()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CloseWebSocketParams {
    pub(super) request_id: String,
    #[serde(default)]
    pub(super) code: Option<u16>,
    #[serde(default)]
    pub(super) reason: String,
}

fn default_url_pattern() -> String {
    "*".to_owned()
}

fn default_request_stage() -> String {
    "Request".to_owned()
}

fn default_websocket_opcode() -> String {
    "Text".to_owned()
}

#[cfg(test)]
mod tests {
    use super::WebSocketMessageOpcode;

    #[test]
    fn websocket_message_opcode_parses_supported_cdp_tokens() {
        assert_eq!(
            WebSocketMessageOpcode::parse("Text"),
            Some(WebSocketMessageOpcode::Text)
        );
        assert_eq!(
            WebSocketMessageOpcode::parse("text"),
            Some(WebSocketMessageOpcode::Text)
        );
        assert_eq!(
            WebSocketMessageOpcode::parse("Binary"),
            Some(WebSocketMessageOpcode::Binary)
        );
        assert_eq!(
            WebSocketMessageOpcode::parse("binary"),
            Some(WebSocketMessageOpcode::Binary)
        );
        assert!(WebSocketMessageOpcode::parse("TEXT").is_none());
        assert!(WebSocketMessageOpcode::parse("Close").is_none());
    }
}
