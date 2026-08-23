//! Client-side response caching for model calls: a [`CachedModel`]
//! decorator implementing `ai_core::Model`, backed by an exact
//! [`TtlCache`] (canonical request hash) and optionally a
//! [`SemanticCache`] (similarity lookup on the last user message via a
//! caller-supplied embedder).
//!
//! Wiring mirrors ai-runtime's `install_resilience` pattern: models are
//! decorated and re-registered through the `register_model` seam, so no
//! changes to `ai-core` are needed (`ai-cache` depends on `ai-core`;
//! the reverse is false — no cycle).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ai_core::{AiClient, AiClientBuilder, ChatRequest, Completion, EventStream, Model, ModelInfo};
use ai_errors::AiError;
use async_trait::async_trait;

use crate::{SemanticCache, TtlCache};

/// FNV-1a 64-bit hash over bytes (stable across platforms; same primitive
/// used by `ai-memory`'s embedders).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Canonical cache key for a [`ChatRequest`].
///
/// The request is serialized to JSON — serde_json's default maps sort
/// object keys, so equal requests produce byte-identical strings regardless
/// of construction order — then FNV-1a hashed. The full JSON string is kept
/// in the key alongside the hash so distinct requests cannot practically
/// collide on both.
pub fn request_cache_key(request: &ChatRequest) -> String {
    let canonical = serde_json::to_string(request).unwrap_or_else(|_| "<unserializable>".into());
    format!("req:{:016x}:{canonical}", fnv1a(canonical.as_bytes()))
}

/// Text of the last user message, used as the semantic-layer lookup text
/// (`None` when the request has no user message).
fn last_user_text(request: &ChatRequest) -> Option<String> {
    request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == ai_core::Role::User)
        .map(ai_core::Message::text_content)
}

/// Embedder for the semantic layer: maps the last user message to its
/// embedding vector.
pub type QueryEmbedder = Arc<dyn Fn(&str) -> Vec<f32> + Send + Sync>;

/// The optional semantic layer of a [`RequestCache`].
#[derive(Clone)]
struct SemanticLayer {
    cache: Arc<SemanticCache>,
    embed: QueryEmbedder,
}

/// A cache for model call results: exact matching always, semantic
/// similarity optionally.
///
/// - **Exact layer** ([`TtlCache`]-backed): keyed by
///   [`request_cache_key`] — messages *and* all sampling parameters.
/// - **Semantic layer** ([`SemanticCache`]-backed, optional): when a hit
///   misses the exact layer, the last user message's embedding (computed by
///   the supplied embedder) is looked up above the similarity threshold.
///   Store-side it records the exact key too, so TTLs stay consistent.
#[derive(Clone)]
pub struct RequestCache {
    exact: TtlCache,
    semantic: Option<SemanticLayer>,
}

impl std::fmt::Debug for RequestCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestCache")
            .field("exact_len", &self.exact.len())
            .field("semantic", &self.semantic.is_some())
            .finish()
    }
}

impl RequestCache {
    /// Exact-only cache (canonical request hash → completion).
    pub fn exact(ttl: Duration, capacity: usize) -> Self {
        Self {
            exact: TtlCache::new(ttl, capacity),
            semantic: None,
        }
    }

    /// Exact + semantic cache. `embed` maps the last user message to its
    /// embedding vector; hits require cosine ≥ `threshold`.
    pub fn with_semantic(
        ttl: Duration,
        capacity: usize,
        threshold: f32,
        embed: QueryEmbedder,
    ) -> Self {
        Self {
            exact: TtlCache::new(ttl, capacity),
            semantic: Some(SemanticLayer {
                cache: Arc::new(SemanticCache::new(ttl, capacity, threshold)),
                embed,
            }),
        }
    }

    /// Looks up a cached completion: exact hash first, then semantic
    /// similarity on `query_text` (when a semantic layer is configured).
    pub fn get(&self, key: &str, query_text: Option<&str>) -> Option<Completion> {
        if let Some(value) = self.exact.get(key) {
            return serde_json::from_value(value).ok();
        }
        if let (Some(layer), Some(text)) = (&self.semantic, query_text) {
            let vector = (layer.embed)(text);
            if let Some((value, _similarity)) = layer.cache.lookup_with(&vector) {
                return serde_json::from_value(value).ok();
            }
        }
        None
    }

    /// Inserts a completion under the exact key and, when configured, into
    /// the semantic index under the query embedding.
    pub fn insert(&self, key: &str, query_text: Option<&str>, completion: &Completion) {
        let value = match serde_json::to_value(completion) {
            Ok(value) => value,
            Err(_) => return, // unserializable completions cannot be cached
        };
        self.exact.set(key.to_string(), value.clone());
        if let (Some(layer), Some(text)) = (&self.semantic, query_text) {
            let vector = (layer.embed)(text);
            if !vector.is_empty() {
                layer.cache.store_embedded(key, value, vector);
            }
        }
    }

    /// Drops every entry of the exact layer. The optional semantic index
    /// keeps its stored vectors (no bulk clear on `SemanticCache`); entries
    /// expire via its TTL like any other cache content.
    pub fn clear(&self) {
        self.exact.clear();
    }

    /// Underlying exact-layer hit/miss counters (diagnostics).
    pub fn exact_stats(&self) -> (u64, u64) {
        self.exact.stats()
    }
}

/// A [`Model`] decorator that serves `generate()` from a [`RequestCache`].
///
/// - Cache key: canonical hash of the whole [`ChatRequest`] (messages +
///   tools + sampling parameters), so e.g. a different temperature misses.
/// - Counters: [`CachedModel::hits`] / [`CachedModel::misses`] expose how
///   many calls were served from cache vs passed through. Cached returns
///   are byte-identical to the original [`Completion`].
/// - **Streaming is never cached**: `stream()` forwards to the inner model
///   untouched (a completed-response cache cannot safely replay partial
///   event streams, and streaming exists precisely to avoid waiting for
///   one).
pub struct CachedModel {
    inner: Arc<dyn Model>,
    cache: RequestCache,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl CachedModel {
    /// Wraps `inner` with an exact-only request cache.
    pub fn new(inner: Arc<dyn Model>, ttl: Duration, capacity: usize) -> Self {
        Self {
            inner,
            cache: RequestCache::exact(ttl, capacity),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Wraps `inner` with a caller-built [`RequestCache`] (e.g. with a
    /// semantic layer).
    pub fn with_cache(inner: Arc<dyn Model>, cache: RequestCache) -> Self {
        Self {
            inner,
            cache,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Number of generate() calls served from the cache.
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Number of generate() calls passed through to the inner model.
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// The decorated cache.
    pub fn cache(&self) -> &RequestCache {
        &self.cache
    }
}

#[async_trait]
impl Model for CachedModel {
    fn info(&self) -> &ModelInfo {
        self.inner.info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let key = request_cache_key(&request);
        let query_text = last_user_text(&request);
        if let Some(completion) = self.cache.get(&key, query_text.as_deref()) {
            tracing::debug!(key = %key, "model response served from cache");
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(completion);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let completion = self.inner.generate(request).await?;
        self.cache.insert(&key, query_text.as_deref(), &completion);
        Ok(completion)
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, AiError> {
        // Pass-through by design: streams are consumed once and are not
        // safely replayable from a completed-response cache.
        self.inner.stream(request).await
    }
}

/// Decorates each listed model reference with a [`CachedModel`] and
/// re-registers it on `client` under the same reference — the ai-cache
/// counterpart of ai-runtime's `install_resilience`.
pub fn install_cache(
    client: &AiClient,
    references: &[&str],
    ttl: Duration,
    capacity: usize,
) -> Result<(), AiError> {
    for reference in references {
        let (_, bare) = client.resolve_model(reference)?;
        client.register_model(*reference, Arc::new(CachedModel::new(bare, ttl, capacity)));
    }
    Ok(())
}

/// Builder-side seam: registers `model` under `reference`, pre-wrapped in a
/// [`CachedModel`], directly on an [`AiClientBuilder`].
pub fn register_cached(
    builder: AiClientBuilder,
    reference: impl Into<String>,
    model: Arc<dyn Model>,
    ttl: Duration,
    capacity: usize,
) -> AiClientBuilder {
    builder.register_model(reference, Arc::new(CachedModel::new(model, ttl, capacity)))
}
