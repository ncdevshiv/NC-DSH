//! Caching: TTL cache, semantic cache interface, and client-side
//! request/response caching for model calls (spec §4.5 cost optimization).
//!
//! The [`model::CachedModel`] decorator wraps any `ai_core::Model` and
//! serves repeated `generate()` calls from a [`model::RequestCache`]
//! (exact canonical-request hashing, optional semantic similarity layer),
//! exposing `hits()`/`misses()` counters. Streaming is passed through by
//! design. Wire it into an `ai_core::AiClient` in one call via
//! [`model::install_cache`] or [`model::register_cached`] — the same
//! register_model decoration seam ai-runtime's resilience uses.

pub mod model;

#[cfg(test)]
mod model_tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

/// A thread-safe TTL cache with bounded size.
///
/// Entries expire lazily on read and are pruned on write when the size cap
/// is exceeded (oldest first). All operations are O(1) amortized.
#[derive(Clone, Debug)]
pub struct TtlCache {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
struct Inner {
    entries: HashMap<String, (serde_json::Value, Instant)>,
    order: Vec<String>,
    ttl: Duration,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl TtlCache {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                entries: HashMap::new(),
                order: Vec::new(),
                ttl,
                capacity: capacity.max(1),
                hits: 0,
                misses: 0,
            })),
        }
    }

    /// Gets a live entry, counting a hit/miss and expiring stale entries.
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        let mut inner = self.inner.write();
        let now = Instant::now();
        let live = inner
            .entries
            .get(key)
            .is_some_and(|(_, expires_at)| *expires_at > now);
        if live {
            inner.hits += 1;
        } else {
            inner.misses += 1;
        }
        // Opportunistic pruning of expired entries.
        if inner.order.len() > 16 {
            let now = Instant::now();
            let live: Vec<String> = inner
                .order
                .iter()
                .filter(|k| inner.entries.get(*k).is_some_and(|(_, t)| *t > now))
                .cloned()
                .collect();
            inner.order = live;
        }
        if live {
            inner.entries.get(key).map(|(value, _)| value.clone())
        } else {
            None
        }
    }

    /// Inserts or refreshes an entry.
    pub fn set(&self, key: impl Into<String>, value: serde_json::Value) {
        let key = key.into();
        let mut inner = self.inner.write();
        let expires_at = Instant::now() + inner.ttl;
        let is_new = !inner.entries.contains_key(&key);
        if is_new {
            inner.order.push(key.clone());
        }
        inner.entries.insert(key, (value, expires_at));
        // Bound memory: evict oldest entries beyond capacity.
        while inner.order.len() > inner.capacity {
            let oldest = inner.order.remove(0);
            inner.entries.remove(&oldest);
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn remove(&self, key: &str) {
        let mut inner = self.inner.write();
        inner.entries.remove(key);
        inner.order.retain(|k| k != key);
    }

    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.entries.clear();
        inner.order.clear();
    }

    pub fn len(&self) -> usize {
        self.inner.read().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hit and miss counters since creation.
    pub fn stats(&self) -> (u64, u64) {
        let inner = self.inner.read();
        (inner.hits, inner.misses)
    }
}

/// Cache hit statistics for cost reporting.
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// A cache keyed by semantic similarity rather than exact string equality
/// (spec §4.2.1 semantic caching).
///
/// The default implementation is an exact-match fallback; embeddings-based
/// lookups plug in via [`SemanticCache::lookup_with`].
/// Stored (embedding, value) pairs.
pub type EmbeddedValue = (Vec<f32>, serde_json::Value);

/// A cache keyed by semantic similarity.
#[derive(Debug, Clone)]
pub struct SemanticCache {
    exact: TtlCache,
    vectors: Arc<RwLock<Vec<EmbeddedValue>>>,
    threshold: f32,
}

impl SemanticCache {
    pub fn new(ttl: Duration, capacity: usize, threshold: f32) -> Self {
        Self {
            exact: TtlCache::new(ttl, capacity),
            vectors: Arc::new(RwLock::new(Vec::new())),
            threshold: threshold.clamp(0.0, 1.0),
        }
    }

    /// Exact-match store (used when embeddings are unavailable).
    pub fn exact(&self) -> &TtlCache {
        &self.exact
    }

    /// Stores a value with its embedding vector.
    pub fn store_embedded(&self, key: &str, value: serde_json::Value, embedding: Vec<f32>) {
        self.exact.set(key, value.clone());
        self.vectors.write().push((embedding, value));
    }

    /// Finds the most similar stored value above `threshold`.
    ///
    /// Returns `(value, similarity)`. `None` when nothing is close enough
    /// or no vectors are stored (then callers should compute embeddings and
    /// store the result).
    pub fn lookup_with(&self, query: &[f32]) -> Option<(serde_json::Value, f32)> {
        let vectors = self.vectors.read();
        vectors
            .iter()
            .filter_map(|(embedding, value)| {
                let similarity = cosine_similarity(query, embedding)?;
                (similarity >= self.threshold).then_some((value.clone(), similarity))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// Cosine similarity of two vectors; `None` for empty/zero vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a.sqrt() * norm_b.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_cache_expires_entries() {
        let cache = TtlCache::new(Duration::from_millis(30), 10);
        cache.set("a", serde_json::json!(1));
        assert!(cache.contains("a"));
        std::thread::sleep(Duration::from_millis(40));
        assert!(!cache.contains("a"));
        assert_eq!(cache.stats().1, 1, "expired read counts as miss");
    }

    #[test]
    fn ttl_cache_bounds_capacity() {
        let cache = TtlCache::new(Duration::from_secs(60), 3);
        for i in 0..6 {
            cache.set(format!("k{i}"), serde_json::json!(i));
        }
        assert!(cache.len() <= 3);
        assert!(!cache.contains("k0"), "oldest evicted");
        assert!(cache.contains("k5"));
    }

    #[test]
    fn ttl_cache_counts_hits_and_misses() {
        let cache = TtlCache::new(Duration::from_secs(60), 10);
        assert!(cache.get("x").is_none());
        cache.set("x", serde_json::json!("v"));
        assert_eq!(cache.get("x"), Some(serde_json::json!("v")));
        let (hits, misses) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn cosine_similarity_identical_and_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert!((cosine_similarity(&a, &b).unwrap() - 1.0).abs() < 1e-6);
        let c = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &c).unwrap() - 0.0).abs() < 1e-6);
        assert!(cosine_similarity(&[], &[]).is_none());
        assert!(cosine_similarity(&[1.0], &[1.0, 2.0]).is_none());
    }

    #[test]
    fn semantic_cache_finds_above_threshold() {
        let cache = SemanticCache::new(Duration::from_secs(60), 10, 0.8);
        cache.store_embedded("q1", serde_json::json!("answer1"), vec![1.0, 0.0]);
        let (value, similarity) = cache.lookup_with(&[0.99, 0.01]).unwrap();
        assert_eq!(value, serde_json::json!("answer1"));
        assert!(similarity > 0.8);
        assert!(
            cache.lookup_with(&[0.0, 1.0]).is_none(),
            "orthogonal query below threshold"
        );
    }

    #[test]
    fn cache_stats_hit_rate() {
        let stats = CacheStats { hits: 3, misses: 1 };
        assert_eq!(stats.hit_rate(), 0.75);
        assert_eq!(CacheStats::default().hit_rate(), 0.0);
    }
}

#[cfg(test)]
mod proptests;
