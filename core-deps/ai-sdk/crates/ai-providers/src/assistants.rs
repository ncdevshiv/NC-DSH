//! Real OpenAI Assistants API v2 client.
//!
//! Provides direct interaction with the OpenAI `/v1/assistants`, `/v1/threads`,
//! `/v1/threads/{id}/messages`, and `/v1/threads/{id}/runs` endpoints using
//! `OpenAI-Beta: assistants=v2`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};
use ai_errors::AiError;

/// An OpenAI Assistant object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantObject {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub model: String,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub tools: Vec<Value>,
}

/// An OpenAI Thread object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadObject {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    #[serde(default)]
    pub metadata: Value,
}

/// An OpenAI Thread Message object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessageObject {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub thread_id: String,
    pub role: String,
    pub content: Vec<Value>,
}

/// An OpenAI Run object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunObject {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub assistant_id: String,
    pub thread_id: String,
    pub status: String,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub failed_at: Option<i64>,
}

/// Client for interacting with the OpenAI Assistants v2 API.
#[derive(Debug, Clone)]
pub struct AssistantsClient {
    api_key: String,
    base_url: String,
    http: HttpClient,
    timeout: Duration,
}

impl AssistantsClient {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, AiError> {
        let http = HttpClient::new()?;
        Ok(Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            http,
            timeout: Duration::from_secs(30),
        })
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn apply_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.api_key)
            .header("OpenAI-Beta", "assistants=v2")
    }

    /// Creates a new assistant (`POST /v1/assistants`).
    pub async fn create_assistant(
        &self,
        model: &str,
        name: Option<&str>,
        instructions: Option<&str>,
    ) -> Result<AssistantObject, AiError> {
        let mut payload = json!({ "model": model });
        if let Some(n) = name {
            payload["name"] = json!(n);
        }
        if let Some(inst) = instructions {
            payload["instructions"] = json!(inst);
        }

        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().post(self.url("/v1/assistants")))
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "assistant.create",
                self.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("assistant.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("assistant.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("assistant.create", &bytes)
    }

    /// Lists assistants (`GET /v1/assistants`).
    pub async fn list_assistants(
        &self,
        limit: Option<u32>,
    ) -> Result<Vec<AssistantObject>, AiError> {
        let mut req = self.apply_headers(self.http.inner().get(self.url("/v1/assistants")));
        if let Some(l) = limit {
            req = req.query(&[("limit", l.to_string())]);
        }

        let response = tokio::time::timeout(self.timeout, req.send())
            .await
            .map_err(|_| {
                AiError::Timeout(ai_errors::TimeoutError::new("assistant.list", self.timeout))
            })?
            .map_err(|e| map_reqwest_error("assistant.list", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("assistant.list", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        let json_val: Value = parse_json("assistant.list", &bytes)?;
        let data = json_val
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(data))
            .map_err(|e| AiError::Serialization(ai_errors::SerializationError::new(e.to_string())))
    }

    /// Creates a new thread (`POST /v1/threads`).
    pub async fn create_thread(&self) -> Result<ThreadObject, AiError> {
        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().post(self.url("/v1/threads")))
                .json(&json!({}))
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("thread.create", self.timeout)))?
        .map_err(|e| map_reqwest_error("thread.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("thread.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("thread.create", &bytes)
    }

    /// Creates a message in a thread (`POST /v1/threads/{thread_id}/messages`).
    pub async fn create_message(
        &self,
        thread_id: &str,
        role: &str,
        content: &str,
    ) -> Result<ThreadMessageObject, AiError> {
        let path = format!("/v1/threads/{thread_id}/messages");
        let payload = json!({
            "role": role,
            "content": content,
        });

        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().post(self.url(&path)))
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new("message.create", self.timeout))
        })?
        .map_err(|e| map_reqwest_error("message.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("message.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("message.create", &bytes)
    }

    /// Runs an assistant on a thread (`POST /v1/threads/{thread_id}/runs`).
    pub async fn create_run(
        &self,
        thread_id: &str,
        assistant_id: &str,
    ) -> Result<RunObject, AiError> {
        let path = format!("/v1/threads/{thread_id}/runs");
        let payload = json!({ "assistant_id": assistant_id });

        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().post(self.url(&path)))
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("run.create", self.timeout)))?
        .map_err(|e| map_reqwest_error("run.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("run.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("run.create", &bytes)
    }

    /// Gets a run by ID (`GET /v1/threads/{thread_id}/runs/{run_id}`).
    pub async fn get_run(&self, thread_id: &str, run_id: &str) -> Result<RunObject, AiError> {
        let path = format!("/v1/threads/{thread_id}/runs/{run_id}");
        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().get(self.url(&path)))
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("run.get", self.timeout)))?
        .map_err(|e| map_reqwest_error("run.get", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("run.get", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("run.get", &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_object_roundtrips_json() {
        let json_data = r#"{
            "id": "asst_123",
            "object": "assistant",
            "created_at": 1700000000,
            "name": "Code Assistant",
            "model": "gpt-4o",
            "instructions": "Help write Rust code",
            "tools": []
        }"#;
        let asst: AssistantObject = serde_json::from_str(json_data).unwrap();
        assert_eq!(asst.id, "asst_123");
        assert_eq!(asst.name.as_deref(), Some("Code Assistant"));
    }
}
