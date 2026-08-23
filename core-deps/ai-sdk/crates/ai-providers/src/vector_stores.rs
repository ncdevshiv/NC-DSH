//! Real OpenAI Vector Stores API client (`OpenAI-Beta: vector_stores=v1`).
//!
//! Implements `POST /v1/vector_stores`, `GET /v1/vector_stores/{id}`, and `DELETE /v1/vector_stores/{id}`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};
use ai_errors::AiError;

/// An OpenAI Vector Store object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreObject {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub usage_bytes: u64,
    #[serde(default)]
    pub file_counts: Value,
    pub status: String,
}

/// Client for OpenAI Vector Stores API endpoints.
#[derive(Debug, Clone)]
pub struct OpenAiVectorStoresClient {
    api_key: String,
    base_url: String,
    http: HttpClient,
    timeout: Duration,
}

impl OpenAiVectorStoresClient {
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

    /// Creates a vector store (`POST /v1/vector_stores`).
    pub async fn create_vector_store(
        &self,
        name: Option<&str>,
        file_ids: Vec<String>,
    ) -> Result<VectorStoreObject, AiError> {
        let mut payload = json!({});
        if let Some(n) = name {
            payload["name"] = json!(n);
        }
        if !file_ids.is_empty() {
            payload["file_ids"] = json!(file_ids);
        }

        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().post(self.url("/v1/vector_stores")))
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "vector_store.create",
                self.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("vector_store.create", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("vector_store.create", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("vector_store.create", &bytes)
    }

    /// Retrieves a vector store by ID (`GET /v1/vector_stores/{id}`).
    pub async fn retrieve_vector_store(
        &self,
        vector_store_id: &str,
    ) -> Result<VectorStoreObject, AiError> {
        let path = format!("/v1/vector_stores/{vector_store_id}");
        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().get(self.url(&path)))
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "vector_store.retrieve",
                self.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("vector_store.retrieve", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("vector_store.retrieve", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("vector_store.retrieve", &bytes)
    }

    /// Deletes a vector store by ID (`DELETE /v1/vector_stores/{id}`).
    pub async fn delete_vector_store(&self, vector_store_id: &str) -> Result<bool, AiError> {
        let path = format!("/v1/vector_stores/{vector_store_id}");
        let response = tokio::time::timeout(
            self.timeout,
            self.apply_headers(self.http.inner().delete(self.url(&path)))
                .send(),
        )
        .await
        .map_err(|_| {
            AiError::Timeout(ai_errors::TimeoutError::new(
                "vector_store.delete",
                self.timeout,
            ))
        })?
        .map_err(|e| map_reqwest_error("vector_store.delete", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("vector_store.delete", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        let val: Value = parse_json("vector_store.delete", &bytes)?;
        Ok(val
            .get("deleted")
            .and_then(|d| d.as_bool())
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_store_object_deserializes() {
        let raw = r#"{
            "id": "vs_123",
            "object": "vector_store",
            "created_at": 1700000000,
            "name": "Docs Index",
            "usage_bytes": 4096,
            "file_counts": {"completed": 2},
            "status": "completed"
        }"#;
        let vs: VectorStoreObject = serde_json::from_str(raw).unwrap();
        assert_eq!(vs.id, "vs_123");
        assert_eq!(vs.name.as_deref(), Some("Docs Index"));
    }
}
