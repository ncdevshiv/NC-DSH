//! Real OpenAI Batch API client.
//!
//! Provides direct interaction with the OpenAI `/v1/batches` endpoint for 50%
//! cost-discounted asynchronous bulk completions.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};
use ai_errors::AiError;

/// Counts of requests in a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRequestCounts {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
}

/// An OpenAI Batch object response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchObject {
    pub id: String,
    pub object: String,
    pub endpoint: String,
    pub input_file_id: String,
    pub completion_window: String,
    pub status: String,
    #[serde(default)]
    pub output_file_id: Option<String>,
    #[serde(default)]
    pub error_file_id: Option<String>,
    pub created_at: i64,
    #[serde(default)]
    pub in_progress_at: Option<i64>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub finalizing_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub failed_at: Option<i64>,
    #[serde(default)]
    pub expired_at: Option<i64>,
    #[serde(default)]
    pub cancelling_at: Option<i64>,
    #[serde(default)]
    pub cancelled_at: Option<i64>,
    #[serde(default)]
    pub request_counts: Option<BatchRequestCounts>,
}

/// Paginated list of Batch objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchListResponse {
    pub object: String,
    pub data: Vec<BatchObject>,
    #[serde(default)]
    pub first_id: Option<String>,
    #[serde(default)]
    pub last_id: Option<String>,
    pub has_more: bool,
}

/// Client for interacting with the OpenAI Batch API.
#[derive(Debug, Clone)]
pub struct OpenAiBatchClient {
    api_key: String,
    base_url: String,
    http: HttpClient,
    timeout: Duration,
}

impl OpenAiBatchClient {
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

    /// Creates a new batch job (`POST /v1/batches`).
    pub async fn create_batch(
        &self,
        input_file_id: &str,
        endpoint: &str,
        completion_window: &str,
    ) -> Result<BatchObject, AiError> {
        let payload = json!({
            "input_file_id": input_file_id,
            "endpoint": endpoint,
            "completion_window": completion_window,
        });

        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .inner()
                .post(self.url("/v1/batches"))
                .bearer_auth(&self.api_key)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("batch.create", self.timeout)))?
        .map_err(|e| map_reqwest_error("batch.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("batch.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("batch.create", &bytes)
    }

    /// Retrieves a batch job by ID (`GET /v1/batches/{batch_id}`).
    pub async fn retrieve_batch(&self, batch_id: &str) -> Result<BatchObject, AiError> {
        let path = format!("/v1/batches/{batch_id}");
        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .inner()
                .get(self.url(&path))
                .bearer_auth(&self.api_key)
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new("batch.retrieve", self.timeout))
        })?
        .map_err(|e| map_reqwest_error("batch.retrieve", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("batch.retrieve", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("batch.retrieve", &bytes)
    }

    /// Cancels an in-flight batch job (`POST /v1/batches/{batch_id}/cancel`).
    pub async fn cancel_batch(&self, batch_id: &str) -> Result<BatchObject, AiError> {
        let path = format!("/v1/batches/{batch_id}/cancel");
        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .inner()
                .post(self.url(&path))
                .bearer_auth(&self.api_key)
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("batch.cancel", self.timeout)))?
        .map_err(|e| map_reqwest_error("batch.cancel", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("batch.cancel", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("batch.cancel", &bytes)
    }

    /// Lists batch jobs (`GET /v1/batches`).
    pub async fn list_batches(
        &self,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<BatchListResponse, AiError> {
        let mut req = self
            .http
            .inner()
            .get(self.url("/v1/batches"))
            .bearer_auth(&self.api_key);
        if let Some(l) = limit {
            req = req.query(&[("limit", l.to_string())]);
        }
        if let Some(a) = after {
            req = req.query(&[("after", a)]);
        }

        let response = tokio::time::timeout(self.timeout, req.send())
            .await
            .map_err(|_| {
                AiError::Timeout(ai_errors::TimeoutError::new("batch.list", self.timeout))
            })?
            .map_err(|e| map_reqwest_error("batch.list", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("batch.list", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("batch.list", &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_object_roundtrips_json() {
        let json_data = r#"{
            "id": "batch_abc123",
            "object": "batch",
            "endpoint": "/v1/chat/completions",
            "input_file_id": "file-xyz789",
            "completion_window": "24h",
            "status": "completed",
            "output_file_id": "file-out123",
            "created_at": 1700000000,
            "completed_at": 1700003600,
            "has_more": false,
            "request_counts": {
                "total": 10,
                "completed": 10,
                "failed": 0
            }
        }"#;
        let batch: BatchObject = serde_json::from_str(json_data).unwrap();
        assert_eq!(batch.id, "batch_abc123");
        assert_eq!(batch.status, "completed");
        assert_eq!(batch.request_counts.unwrap().completed, 10);
    }
}
