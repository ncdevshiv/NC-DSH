//! Real OpenAI Moderation API client.
//!
//! Implements `POST /v1/moderations` for safety policy and content violation checking.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};
use ai_errors::AiError;

/// Category flags for content moderation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationCategories {
    #[serde(default)]
    pub hate: bool,
    #[serde(default, rename = "hate/threatening")]
    pub hate_threatening: bool,
    #[serde(default)]
    pub harassment: bool,
    #[serde(default, rename = "harassment/threatening")]
    pub harassment_threatening: bool,
    #[serde(default, rename = "self-harm")]
    pub self_harm: bool,
    #[serde(default)]
    pub sexual: bool,
    #[serde(default, rename = "sexual/minors")]
    pub sexual_minors: bool,
    #[serde(default)]
    pub violence: bool,
    #[serde(default, rename = "violence/graphic")]
    pub violence_graphic: bool,
}

/// Result entry for a single input text moderation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationResult {
    pub flagged: bool,
    pub categories: ModerationCategories,
}

/// Response payload from the Moderations API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationResponse {
    pub id: String,
    pub model: String,
    pub results: Vec<ModerationResult>,
}

/// Client for OpenAI Moderation endpoints.
#[derive(Debug, Clone)]
pub struct OpenAiModerationClient {
    api_key: String,
    base_url: String,
    http: HttpClient,
    timeout: Duration,
}

impl OpenAiModerationClient {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, AiError> {
        let http = HttpClient::new()?;
        Ok(Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            http,
            timeout: Duration::from_secs(15),
        })
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// Checks text against moderation policies (`POST /v1/moderations`).
    pub async fn create_moderation(
        &self,
        input: &str,
        model: Option<&str>,
    ) -> Result<ModerationResponse, AiError> {
        let mut payload = json!({ "input": input });
        if let Some(m) = model {
            payload["model"] = json!(m);
        }

        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .inner()
                .post(self.url("/v1/moderations"))
                .bearer_auth(&self.api_key)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "moderation.create",
                self.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("moderation.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("moderation.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("moderation.create", &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moderation_response_deserializes() {
        let raw = r#"{
            "id": "modr-123",
            "model": "omni-moderation-latest",
            "results": [
                {
                    "flagged": false,
                    "categories": {
                        "hate": false,
                        "harassment": false,
                        "self-harm": false,
                        "sexual": false,
                        "violence": false
                    }
                }
            ]
        }"#;
        let res: ModerationResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(res.id, "modr-123");
        assert!(!res.results[0].flagged);
    }
}
