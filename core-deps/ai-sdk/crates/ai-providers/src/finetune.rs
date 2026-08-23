//! Real OpenAI fine-tuning jobs API client (spec §3: fine-tuning adapter).
//!
//! Implements `POST /fine_tuning/jobs`, `GET /fine_tuning/jobs`,
//! `GET /fine_tuning/jobs/{id}`, `POST /fine_tuning/jobs/{id}/cancel`,
//! `GET /fine_tuning/jobs/{id}/events`, and training-file upload via
//! `POST /files`. Requires an OpenAI-compatible API key. Local LoRA
//! training is intentionally out of scope (documented).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use ai_errors::{AiError, SerializationError, TimeoutError};

use crate::http::{
    HttpClient, map_reqwest_error, map_response_error, parse_json, retry_after_from_headers,
};

/// A fine-tuning job (OpenAI wire shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningJob {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub training_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FineTuningError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trained_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
}

/// A job error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningError {
    pub code: String,
    pub message: String,
}

/// A fine-tuning event (job progress).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningEvent {
    pub id: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub created_at: u64,
}

/// Hyperparameters for a fine-tuning job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hyperparameters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_epochs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learning_rate_multiplier: Option<f64>,
}

/// The OpenAI-compatible fine-tuning client (real HTTP).
#[derive(Clone)]
pub struct FineTuningClient {
    base_url: String,
    api_key: String,
    http: HttpClient,
    timeout: Duration,
}

impl FineTuningClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, AiError> {
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            http: HttpClient::shared(),
            timeout: Duration::from_secs(60),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }

    async fn get(&self, path: &str) -> Result<Value, AiError> {
        let response = tokio::time::timeout(
            self.timeout,
            self.http
                .execute(self.http.get(self.url(path)).bearer_auth(&self.api_key)),
        )
        .await
        .map_err(|_| TimeoutError::new("finetune.get", self.timeout))?
        .map_err(|e| map_reqwest_error("finetune.get", e))?;
        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("finetune.get", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }
        parse_json("finetune.get", &bytes)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, AiError> {
        let response = tokio::time::timeout(
            self.timeout,
            self.http.execute(
                self.http
                    .post(self.url(path))
                    .bearer_auth(&self.api_key)
                    .json(&body),
            ),
        )
        .await
        .map_err(|_| TimeoutError::new("finetune.post", self.timeout))?
        .map_err(|e| map_reqwest_error("finetune.post", e))?;
        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("finetune.post", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }
        parse_json("finetune.post", &bytes)
    }

    /// Uploads a JSONL training file (`POST /files`) and returns the file id.
    pub async fn upload_training_file(
        &self,
        filename: &str,
        jsonl: &str,
    ) -> Result<String, AiError> {
        let response = tokio::time::timeout(
            self.timeout,
            self.http.execute(
                self.http
                    .post(self.url("files"))
                    .bearer_auth(&self.api_key)
                    .multipart(
                        reqwest::multipart::Form::new()
                            .text("purpose", "fine-tune".to_string())
                            .part(
                                "file",
                                reqwest::multipart::Part::bytes(jsonl.as_bytes().to_vec())
                                    .file_name(filename.to_string())
                                    .mime_str("application/jsonl")
                                    .map_err(|e| {
                                        AiError::Web(ai_errors::WebError::new(
                                            "finetune.upload",
                                            e.to_string(),
                                        ))
                                    })?,
                            ),
                    ),
            ),
        )
        .await
        .map_err(|_| TimeoutError::new("finetune.upload", self.timeout))?
        .map_err(|e| map_reqwest_error("finetune.upload", e))?;
        let status = response.status();
        let retry_after = retry_after_from_headers(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_reqwest_error("finetune.upload", e))?
            .to_vec();
        if !status.is_success() {
            return Err(map_response_error("openai", status, retry_after, &bytes).await);
        }
        let json: Value = parse_json("finetune.upload", &bytes)?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                AiError::Serialization(SerializationError::new("file upload response missing `id`"))
            })
    }

    /// Creates a fine-tuning job.
    pub async fn create_job(
        &self,
        model: &str,
        training_file: &str,
        hyperparameters: Hyperparameters,
    ) -> Result<FineTuningJob, AiError> {
        let mut body = serde_json::json!({
            "model": model,
            "training_file": training_file,
        });
        let hyper = serde_json::to_value(&hyperparameters)
            .map_err(|e| SerializationError::new(e.to_string()))?;
        if !hyper.as_object().map(|h| h.is_empty()).unwrap_or(true) {
            body["hyperparameters"] = hyper;
        }
        let json = self.post("fine_tuning/jobs", body).await?;
        serde_json::from_value(json)
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))
    }

    /// Lists fine-tuning jobs.
    pub async fn list_jobs(&self, limit: Option<u64>) -> Result<Vec<FineTuningJob>, AiError> {
        let path = match limit {
            Some(limit) => format!("fine_tuning/jobs?limit={limit}"),
            None => "fine_tuning/jobs".to_string(),
        };
        let json = self.get(&path).await?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(data))
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))
    }

    /// Gets a job by id.
    pub async fn get_job(&self, job_id: &str) -> Result<FineTuningJob, AiError> {
        let json = self.get(&format!("fine_tuning/jobs/{job_id}")).await?;
        serde_json::from_value(json)
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))
    }

    /// Cancels a job.
    pub async fn cancel_job(&self, job_id: &str) -> Result<FineTuningJob, AiError> {
        let json = self
            .post(
                &format!("fine_tuning/jobs/{job_id}/cancel"),
                serde_json::json!({}),
            )
            .await?;
        serde_json::from_value(json)
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))
    }

    /// Lists a job's events.
    pub async fn list_events(
        &self,
        job_id: &str,
        limit: Option<u64>,
    ) -> Result<Vec<FineTuningEvent>, AiError> {
        let path = match limit {
            Some(limit) => format!("fine_tuning/jobs/{job_id}/events?limit={limit}"),
            None => format!("fine_tuning/jobs/{job_id}/events"),
        };
        let json = self.get(&path).await?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(data))
            .map_err(|e| AiError::Serialization(SerializationError::new(e.to_string())))
    }
}

/// The `Value` alias used by the client methods.
type Value = serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn job_wire_roundtrip() {
        let job_json = json!({
            "id": "ftjob-abc123",
            "model": "gpt-4o-mini-2024-07-18",
            "status": "succeeded",
            "training_file": "file-xyz",
            "validation_file": null,
            "error": null,
            "trained_tokens": 1234,
            "finished_at": 1720000000,
            "created_at": 1719990000
        });
        let job: FineTuningJob = serde_json::from_value(job_json).unwrap();
        assert_eq!(job.id, "ftjob-abc123");
        assert_eq!(job.status, "succeeded");
        assert_eq!(job.trained_tokens, Some(1234));
        assert!(job.error.is_none());

        let back: Value = serde_json::to_value(&job).unwrap();
        assert_eq!(back["id"], "ftjob-abc123");
    }

    #[test]
    fn event_wire_roundtrip() {
        let event_json = json!({
            "id": "ftev-1",
            "level": "info",
            "message": "Created fine-tuning job",
            "data": {"step": 1},
            "created_at": 1719990000
        });
        let event: FineTuningEvent = serde_json::from_value(event_json).unwrap();
        assert_eq!(event.message, "Created fine-tuning job");
        assert_eq!(event.data["step"], 1);
    }

    #[test]
    fn hyperparameters_serialize_only_when_set() {
        let empty = Hyperparameters::default();
        let value = serde_json::to_value(&empty).unwrap();
        assert!(value.as_object().unwrap().is_empty());

        let set = Hyperparameters {
            n_epochs: Some(3),
            batch_size: None,
            learning_rate_multiplier: Some(1.5),
        };
        let value = serde_json::to_value(&set).unwrap();
        assert_eq!(value["n_epochs"], 3);
        assert_eq!(value["learning_rate_multiplier"], 1.5);
        assert!(value.get("batch_size").is_none());
    }

    #[test]
    fn url_building_joins_base() {
        let client = FineTuningClient::new("https://api.openai.com/v1", "sk-test").unwrap();
        assert_eq!(
            client.url("fine_tuning/jobs"),
            "https://api.openai.com/v1/fine_tuning/jobs"
        );
        // Trailing slash is normalized.
        let client = FineTuningClient::new("https://api.openai.com/v1/", "sk-test").unwrap();
        assert_eq!(
            client.url("fine_tuning/jobs"),
            "https://api.openai.com/v1/fine_tuning/jobs"
        );
    }
}
