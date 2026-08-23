//! Real OpenAI Files Management API client.
//!
//! Implements `GET /v1/files`, `GET /v1/files/{id}`, and `DELETE /v1/files/{id}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};
use ai_errors::AiError;

/// An OpenAI File object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileObject {
    pub id: String,
    pub object: String,
    pub bytes: u64,
    pub created_at: i64,
    pub filename: String,
    pub purpose: String,
}

/// Client for OpenAI Files API endpoints.
#[derive(Debug, Clone)]
pub struct OpenAiFilesClient {
    api_key: String,
    base_url: String,
    http: HttpClient,
    timeout: Duration,
}

impl OpenAiFilesClient {
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

    /// Lists uploaded files (`GET /v1/files`).
    pub async fn list_files(&self, purpose: Option<&str>) -> Result<Vec<FileObject>, AiError> {
        let mut req = self
            .http
            .inner()
            .get(self.url("/v1/files"))
            .bearer_auth(&self.api_key);
        if let Some(p) = purpose {
            req = req.query(&[("purpose", p)]);
        }

        let response = tokio::time::timeout(self.timeout, req.send())
            .await
            .map_err(|_| {
                AiError::Timeout(ai_errors::TimeoutError::new("files.list", self.timeout))
            })?
            .map_err(|e| map_reqwest_error("files.list", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("files.list", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        let json_val: Value = parse_json("files.list", &bytes)?;
        let data = json_val
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(data))
            .map_err(|e| AiError::Serialization(ai_errors::SerializationError::new(e.to_string())))
    }

    /// Retrieves file metadata by ID (`GET /v1/files/{file_id}`).
    pub async fn retrieve_file(&self, file_id: &str) -> Result<FileObject, AiError> {
        let path = format!("/v1/files/{file_id}");
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
            AiError::Timeout(ai_errors::TimeoutError::new("files.retrieve", self.timeout))
        })?
        .map_err(|e| map_reqwest_error("files.retrieve", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("files.retrieve", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        parse_json("files.retrieve", &bytes)
    }

    /// Deletes a file by ID (`DELETE /v1/files/{file_id}`).
    pub async fn delete_file(&self, file_id: &str) -> Result<bool, AiError> {
        let path = format!("/v1/files/{file_id}");
        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .inner()
                .delete(self.url(&path))
                .bearer_auth(&self.api_key)
                .send(),
        )
        .await
        .map_err(|_| AiError::Timeout(ai_errors::TimeoutError::new("files.delete", self.timeout)))?
        .map_err(|e| map_reqwest_error("files.delete", e))?;

        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("files.delete", e))?
            .to_vec();

        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }

        let val: Value = parse_json("files.delete", &bytes)?;
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
    fn file_object_deserializes() {
        let raw = r#"{
            "id": "file-123",
            "object": "file",
            "bytes": 1024,
            "created_at": 1700000000,
            "filename": "data.jsonl",
            "purpose": "fine-tune"
        }"#;
        let file: FileObject = serde_json::from_str(raw).unwrap();
        assert_eq!(file.id, "file-123");
        assert_eq!(file.filename, "data.jsonl");
    }
}
