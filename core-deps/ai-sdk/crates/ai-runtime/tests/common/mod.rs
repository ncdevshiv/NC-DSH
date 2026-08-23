#![allow(dead_code)] // shared by several test binaries; not every helper is used everywhere

//! Shared fixtures for the AEGIS chaos-proof integration tests: a minimal
//! reqwest-backed [`Model`] pointed at the in-crate chaos server, plus a
//! scripted model for deterministic fallback scenarios.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use ai_core::{ChatRequest, Completion, EventStream, Message, Model, ModelInfo, Role};
use ai_errors::{
    AiError, InternalError, NetworkError, RateLimitError, SerializationError, TimeoutError,
};
use ai_types::{ModelId, ProviderId};

/// A `Model` speaking plain HTTP/1.1 JSON against the chaos server's
/// `/v1/chat/completions` endpoint — the same shape a provider adapter uses,
/// so the decorator is exercised against *real* transport failures
/// (dropped connections, stalls, 5xx/429, malformed payloads).
pub struct HttpModel {
    info: ModelInfo,
    url: String,
    http: reqwest::Client,
}

impl HttpModel {
    pub fn new(base_url: &str, model_id: &str) -> Self {
        // No client-wide timeout: deadlines are owned by ResiliencePolicy.
        let http = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client builds");
        Self {
            info: ModelInfo::new(
                ProviderId::new("chaos"),
                ModelId::new(model_id),
                128_000,
                8_192,
            )
            .with_name(model_id),
            url: format!("{base_url}/v1/chat/completions"),
            http,
        }
    }

    pub fn arc(base_url: &str, model_id: &str) -> Arc<Self> {
        Arc::new(Self::new(base_url, model_id))
    }
}

fn map_transport_error(operation: &str, error: reqwest::Error) -> AiError {
    if error.is_timeout() {
        AiError::Timeout(TimeoutError::new(operation, Duration::ZERO))
    } else {
        AiError::Network(NetworkError::new(operation, error.to_string()))
    }
}

fn provider_error(provider: &str, status: reqwest::StatusCode, message: String) -> AiError {
    AiError::Provider(ai_errors::ProviderError::new(provider, message).with_status(status.as_u16()))
}

#[async_trait]
impl Model for HttpModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        let operation = "chaos.generate";
        let body = serde_json::json!({
            "model": self.info.id.as_str(),
            "messages": [{"role": "user", "content": "ping"}],
        });

        let response = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| map_transport_error(operation, e))?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| map_transport_error(operation, e))?;

        if status.as_u16() == 429 {
            return Err(AiError::RateLimit({
                let mut rl = RateLimitError::new("chaos", "rate limited by chaos server");
                rl.retry_after = retry_after.or(Some(Duration::from_secs(0)));
                rl
            }));
        }
        if status.is_server_error() {
            return Err(provider_error(
                "chaos",
                status,
                format!("server fault: {}", String::from_utf8_lossy(&bytes)),
            ));
        }
        if !status.is_success() {
            return Err(provider_error(
                "chaos",
                status,
                "unexpected non-success status".to_string(),
            ));
        }

        // A 200 whose body does not parse is classified as a transient bad
        // gateway (provider 502): the upstream handed us an unusable payload,
        // which is an infrastructure fault worth retrying — not a caller bug.
        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
            AiError::Provider(
                ai_errors::ProviderError::new("chaos", "upstream returned an unparseable payload")
                    .with_status(502),
            )
        })?;

        let text = json
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AiError::Serialization(SerializationError::new(
                    "completion JSON missing choices[0].message.content",
                ))
            })?
            .to_string();

        Ok(Completion {
            provider: self.info.provider.clone(),
            model: self.info.id.clone(),
            text,
            tool_calls: Vec::new(),
            usage: Default::default(),
            reasoning: None,
            raw: json,
            finish_reason: Some("stop".into()),
        })
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        // The chaos suite exercises streaming through scripted models; the
        // HTTP double only needs to serve completions.
        Err(AiError::Internal(InternalError::new(
            "HttpModel does not implement streaming",
        )))
    }
}

/// A fully scripted model for deterministic fallback / breaker scenarios.
pub struct ScriptedModel {
    info: ModelInfo,
    /// `None` = succeed forever with `ok_text`; `Some(msg)` = fail forever.
    behavior: Option<String>,
    ok_text: &'static str,
    calls: AtomicU32,
}

impl ScriptedModel {
    pub fn always_ok(ok_text: &'static str) -> Arc<Self> {
        Arc::new(Self {
            info: ModelInfo::new(
                ProviderId::new("scripted"),
                ModelId::new(ok_text),
                128_000,
                8_192,
            ),
            behavior: None,
            ok_text,
            calls: AtomicU32::new(0),
        })
    }

    pub fn always_fail(reason: &'static str) -> Arc<Self> {
        Arc::new(Self {
            info: ModelInfo::new(
                ProviderId::new("scripted"),
                ModelId::new("failing"),
                128_000,
                8_192,
            ),
            behavior: Some(reason.to_string()),
            ok_text: "",
            calls: AtomicU32::new(0),
        })
    }

    pub fn call_count(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Model for ScriptedModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match &self.behavior {
            Some(reason) => Err(AiError::Network(NetworkError::new(
                "scripted",
                reason.clone(),
            ))),
            None => Ok(Completion {
                provider: self.info.provider.clone(),
                model: self.info.id.clone(),
                text: self.ok_text.to_string(),
                tool_calls: Vec::new(),
                usage: Default::default(),
                reasoning: None,
                raw: serde_json::Value::Null,
                finish_reason: Some("stop".into()),
            }),
        }
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        match &self.behavior {
            Some(reason) => Err(AiError::Network(NetworkError::new(
                "scripted",
                reason.clone(),
            ))),
            None => Ok(Box::pin(futures::stream::iter(vec![Ok(
                ai_core::StreamEvent::Completed {
                    finish_reason: Some("stop".into()),
                },
            )]))),
        }
    }
}

/// A one-message chat request.
pub fn ping_request() -> ChatRequest {
    ChatRequest::new(vec![Message::text(Role::User, "ping")])
}

/// Percentile of a sample (nearest-rank on the sorted vector).
pub fn percentile(samples: &mut [u128], pct: f64) -> u128 {
    assert!(!samples.is_empty(), "percentile of empty sample");
    samples.sort_unstable();
    let rank = ((samples.len() as f64) * pct / 100.0).ceil();
    let index = rank.max(1.0) as usize - 1;
    samples[index.min(samples.len() - 1)]
}
