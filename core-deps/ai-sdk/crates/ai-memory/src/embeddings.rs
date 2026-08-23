//! Embeddings providers: a real OpenAI-compatible `/embeddings` adapter and
//! the trait semantic memory uses.

use std::time::Duration;

use async_trait::async_trait;

use ai_errors::AiError;

/// An embeddings failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingsError(pub String);

impl std::fmt::Display for EmbeddingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EmbeddingsError {}

impl From<EmbeddingsError> for AiError {
    fn from(e: EmbeddingsError) -> Self {
        AiError::Internal(ai_errors::InternalError::new(e.0))
    }
}

/// Produces embedding vectors for text (used by semantic memory / RAG).
#[async_trait]
pub trait EmbeddingsProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError>;

    /// Feeds texts to the provider *before* they are embedded so stateful
    /// providers can learn from them (e.g. [`crate::NgramEmbeddings`]
    /// accumulates online document frequencies for idf weighting). Called
    /// by ingest paths such as the RAG pipeline. Default: no-op, which is
    /// exactly right for stateless providers.
    async fn observe(&self, _texts: &[String]) {}
}

/// Real OpenAI-compatible embeddings adapter (`POST {base}/embeddings`).
///
/// Works against OpenAI, OpenRouter-compatible gateways, and the project
/// gateway when it exposes the endpoint. Requires an API key.
pub struct OpenAiCompatEmbeddings {
    base_url: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAiCompatEmbeddings {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, EmbeddingsError> {
        let client = reqwest::Client::builder()
            .user_agent("ai-sdk/0.1")
            .build()
            .map_err(|e| EmbeddingsError(format!("client build failed: {e}")))?;
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            client,
            timeout: Duration::from_secs(30),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl EmbeddingsProvider for OpenAiCompatEmbeddings {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let response = tokio::time::timeout(
            self.timeout,
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&serde_json::json!({
                    "model": self.model,
                    "input": texts
                }))
                .send(),
        )
        .await
        .map_err(|_| EmbeddingsError("embeddings request timed out".into()))?
        .map_err(|e| EmbeddingsError(format!("embeddings request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| String::new());
            return Err(EmbeddingsError(format!("embeddings HTTP {status}: {body}")));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| EmbeddingsError(format!("invalid embeddings response: {e}")))?;

        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| EmbeddingsError("embeddings response missing `data`".into()))?;

        let mut vectors = Vec::with_capacity(data.len());
        for item in data {
            let vector: Vec<f32> = item
                .get("embedding")
                .and_then(|e| serde_json::from_value(e.clone()).ok())
                .ok_or_else(|| {
                    EmbeddingsError("embedding entry missing/invalid `embedding`".into())
                })?;
            vectors.push(vector);
        }
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_input_returns_empty() {
        let provider = OpenAiCompatEmbeddings::new(
            "https://example.invalid/v1",
            "sk-test",
            "text-embedding-3-small",
        )
        .unwrap();
        assert!(provider.embed(&[]).await.unwrap().is_empty());
    }
}
