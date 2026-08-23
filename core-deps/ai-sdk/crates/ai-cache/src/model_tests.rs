//! Tests for the [`CachedModel`] decorator and its client wiring.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ai_core::{AiClient, ChatRequest, Completion, EventStream, Model, ModelInfo};
use ai_types::{ModelId, ProviderId, Usage};
use async_trait::async_trait;

use crate::model::{CachedModel, RequestCache, install_cache, register_cached, request_cache_key};

/// A mock model counting generate/stream invocations.
#[derive(Debug)]
struct CountingModel {
    info: ModelInfo,
    generates: AtomicU64,
    streams: AtomicU64,
}

impl CountingModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            info: ModelInfo::new(
                ProviderId::new("mock"),
                ModelId::new("counting"),
                128_000,
                8_192,
            ),
            generates: AtomicU64::new(0),
            streams: AtomicU64::new(0),
        })
    }

    fn generate_count(&self) -> u64 {
        self.generates.load(Ordering::SeqCst)
    }
}

fn completion_for(info: &ModelInfo, text: &str) -> Completion {
    Completion {
        provider: info.provider.clone(),
        model: info.id.clone(),
        text: text.to_string(),
        tool_calls: Vec::new(),
        usage: Usage::new(1, 1),
        reasoning: None,
        raw: serde_json::Value::Null,
        finish_reason: Some("stop".into()),
    }
}

#[async_trait]
impl Model for CountingModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        self.generates.fetch_add(1, Ordering::SeqCst);
        Ok(completion_for(&self.info, "cached answer"))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, ai_errors::AiError> {
        use futures_core::Stream;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        self.streams.fetch_add(1, Ordering::SeqCst);
        struct Empty;
        impl Stream for Empty {
            type Item = Result<ai_core::StreamEvent, ai_errors::AiError>;
            fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                Poll::Ready(None)
            }
        }
        Ok(Box::pin(Empty))
    }
}

use ai_errors::AiError;

fn request(text: &str) -> ChatRequest {
    ChatRequest::new(vec![ai_core::Message::text(ai_core::Role::User, text)])
}

#[tokio::test]
async fn repeated_generate_hits_cache_and_counters_prove_it() {
    let inner = CountingModel::new();
    let cached = CachedModel::new(inner.clone() as Arc<dyn Model>, Duration::from_secs(60), 16);

    let first = cached.generate(request("hello")).await.unwrap();
    let second = cached.generate(request("hello")).await.unwrap();

    assert_eq!(first.text, second.text, "cache returns the same completion");
    assert_eq!(inner.generate_count(), 1, "inner model called exactly once");
    assert_eq!(cached.hits(), 1);
    assert_eq!(cached.misses(), 1);

    // A third identical call also hits.
    let _ = cached.generate(request("hello")).await.unwrap();
    assert_eq!(inner.generate_count(), 1);
    assert_eq!(cached.hits(), 2);
    assert_eq!(cached.misses(), 1);
}

#[tokio::test]
async fn different_temperature_misses_cache() {
    let inner = CountingModel::new();
    let cached = CachedModel::new(inner.clone() as Arc<dyn Model>, Duration::from_secs(60), 16);

    let cold = request("hello").with_temperature(0.0);
    let warm = request("hello").with_temperature(0.9);
    let _ = cached.generate(cold).await.unwrap();
    let _ = cached.generate(warm).await.unwrap();

    assert_eq!(
        inner.generate_count(),
        2,
        "sampling params are part of the key"
    );
    assert_eq!(cached.hits(), 0);
    assert_eq!(cached.misses(), 2);

    // Repeating one of them hits again.
    let _ = cached
        .generate(request("hello").with_temperature(0.9))
        .await
        .unwrap();
    assert_eq!(inner.generate_count(), 2);
    assert_eq!(cached.hits(), 1);
}

#[tokio::test]
async fn cache_respects_ttl_expiry() {
    let inner = CountingModel::new();
    let cached = CachedModel::new(
        inner.clone() as Arc<dyn Model>,
        Duration::from_millis(50),
        16,
    );

    let _ = cached.generate(request("hello")).await.unwrap(); // miss
    let _ = cached.generate(request("hello")).await.unwrap(); // hit
    tokio::time::sleep(Duration::from_millis(80)).await;
    let _ = cached.generate(request("hello")).await.unwrap(); // expired → miss

    assert_eq!(inner.generate_count(), 2, "expired entry re-generates");
    assert_eq!(cached.hits(), 1);
    assert_eq!(cached.misses(), 2);
}

#[tokio::test]
async fn streaming_passes_through_and_never_touches_cache() {
    let inner = CountingModel::new();
    let cached = CachedModel::new(inner.clone() as Arc<dyn Model>, Duration::from_secs(60), 16);

    let _ = cached.stream(request("hello")).await.unwrap();
    let _ = cached.stream(request("hello")).await.unwrap();

    assert_eq!(
        inner.streams.load(Ordering::SeqCst),
        2,
        "stream calls forward to the inner model"
    );
    assert_eq!(cached.hits(), 0, "streaming is never cached");
    assert_eq!(cached.misses(), 0, "streaming bypasses generate counters");
    assert_eq!(inner.generate_count(), 0);
}

#[tokio::test]
async fn semantic_layer_serves_similar_prompt_from_cache() {
    let inner = CountingModel::new();

    // Toy embedder: normalized bag of letters over a fixed alphabet —
    // near-identical prompts get cosine ≈ 1, unrelated ones stay low.
    let embed = |text: &str| {
        let mut v = vec![0.0f32; 8];
        for ch in text.chars() {
            if ch.is_ascii_alphabetic() {
                let slot = (ch.to_ascii_lowercase() as u8 - b'a') % 8;
                v[slot as usize] += 1.0;
            }
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    };
    let cache = RequestCache::with_semantic(
        Duration::from_secs(60),
        16,
        0.95,
        Arc::new(move |t: &str| embed(t)),
    );
    let cached = CachedModel::with_cache(inner.clone() as Arc<dyn Model>, cache);

    let _ = cached
        .generate(request("buy fresh apples today"))
        .await
        .unwrap();
    // Same words modulo punctuation/case → above threshold semantically.
    let _ = cached
        .generate(request("Buy FRESH apples today!"))
        .await
        .unwrap();

    assert_eq!(
        inner.generate_count(),
        1,
        "semantic layer served the variant"
    );
    assert_eq!(cached.hits(), 1);
    assert_eq!(cached.misses(), 1);
}

#[tokio::test]
async fn install_cache_wires_client_end_to_end() {
    let inner = CountingModel::new();
    let client = AiClient::builder().build().unwrap();
    client.register_model("mock:counting", inner.clone() as Arc<dyn Model>);

    install_cache(&client, &["mock:counting"], Duration::from_secs(60), 16).unwrap();

    let messages = vec![ai_core::Message::text(ai_core::Role::User, "ping")];
    let a = client
        .generate("mock:counting", messages.clone())
        .await
        .unwrap();
    let b = client.generate("mock:counting", messages).await.unwrap();
    assert_eq!(a.text, b.text);
    assert_eq!(
        inner.generate_count(),
        1,
        "second client call came from cache"
    );
}

#[tokio::test]
async fn register_cached_wraps_on_builder() {
    let inner = CountingModel::new();
    let client = register_cached(
        AiClient::builder(),
        "mock:counting",
        inner.clone() as Arc<dyn Model>,
        Duration::from_secs(60),
        16,
    )
    .build()
    .unwrap();

    let messages = vec![ai_core::Message::text(ai_core::Role::User, "ping")];
    let _ = client
        .generate("mock:counting", messages.clone())
        .await
        .unwrap();
    let _ = client.generate("mock:counting", messages).await.unwrap();
    assert_eq!(inner.generate_count(), 1);
}

#[test]
fn cache_key_is_canonical_across_field_order() {
    let a = request("hello").with_temperature(0.5).with_seed(7);
    // Constructing an equal request through a different builder order must
    // yield the identical canonical key (serde_json sorts object keys).
    let mut b = request("hello");
    b.seed = Some(7);
    b.temperature = Some(0.5);
    assert_eq!(request_cache_key(&a), request_cache_key(&b));

    let c = request("hello").with_temperature(0.6);
    assert_ne!(request_cache_key(&a), request_cache_key(&c));
}
