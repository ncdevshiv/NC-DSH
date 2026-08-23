//! Self-hosted statistical embeddings: deterministic feature hashing with
//! term-frequency weighting and L2 normalization.
//!
//! No external service, no model downloads — a real, widely-used technique
//! for semantic similarity of short-to-medium text (feature hashing /
//! hashing trick). Good enough for retrieval baselines and for fully
//! self-hosted pipelines where the gateway does not expose `/embeddings`.

use crate::embeddings::{EmbeddingsError, EmbeddingsProvider};

/// Configuration for [`StatisticalEmbeddings`].
#[derive(Debug, Clone)]
pub struct StatisticalConfig {
    /// Vector dimensions (power of two keeps the hash distribution even).
    pub dimensions: usize,
    /// Minimum token length to include.
    pub min_token_len: usize,
}

impl Default for StatisticalConfig {
    fn default() -> Self {
        Self {
            dimensions: 512,
            min_token_len: 2,
        }
    }
}

/// Feature-hashing embeddings computed locally.
pub struct StatisticalEmbeddings {
    config: StatisticalConfig,
}

impl StatisticalEmbeddings {
    pub fn new(config: StatisticalConfig) -> Self {
        Self {
            config: StatisticalConfig {
                dimensions: config.dimensions.next_power_of_two().max(16),
                ..config
            },
        }
    }

    /// A default configuration instance.
    pub fn defaults() -> Self {
        Self::new(StatisticalConfig::default())
    }

    /// Tokenizes text into lowercase word stems (reused from the RAG
    /// tokenizer semantics: alphanumeric runs).
    fn tokens(&self, text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= self.config.min_token_len)
            .map(|t| t.to_lowercase())
            .collect()
    }

    /// FNV-1a 64-bit hash of a string (stable across platforms).
    fn fnv1a(input: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in input.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Embeds one text: signed feature hashing + log term frequency,
    /// L2-normalized.
    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.config.dimensions];
        let mut term_freq: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for token in self.tokens(text) {
            *term_freq.entry(token).or_insert(0) += 1;
        }
        for (token, freq) in term_freq {
            let hash = Self::fnv1a(&token);
            let index = (hash as usize) & (self.config.dimensions - 1);
            let sign = if hash & 0x8000_0000_0000_0000 == 0 {
                1.0
            } else {
                -1.0
            };
            let weight = 1.0 + (freq as f32).ln();
            vector[index] += sign * weight;
        }
        // L2 normalize.
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

#[async_trait::async_trait]
impl EmbeddingsProvider for StatisticalEmbeddings {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn similar_texts_are_more_similar_than_dissimilar() {
        let embeddings = StatisticalEmbeddings::defaults();
        let vectors = embeddings
            .embed(&[
                "the quick brown fox jumps over the lazy dog".to_string(),
                "a quick brown fox leaps over a sleepy dog".to_string(),
                "the stock market opened flat on tuesday morning".to_string(),
            ])
            .await
            .unwrap();

        let similar = ai_cache::cosine_similarity(&vectors[0], &vectors[1]).unwrap_or(0.0);
        let dissimilar = ai_cache::cosine_similarity(&vectors[0], &vectors[2]).unwrap_or(0.0);
        assert!(
            similar > dissimilar,
            "similar={similar} dissimilar={dissimilar}"
        );
        assert!(
            similar > 0.3,
            "similar texts should be reasonably close: {similar}"
        );
        assert!(
            dissimilar < 0.5,
            "dissimilar texts should be far: {dissimilar}"
        );
    }

    #[tokio::test]
    async fn identical_texts_have_near_unit_similarity() {
        let embeddings = StatisticalEmbeddings::defaults();
        let vectors = embeddings
            .embed(&["hello world".to_string(), "hello world".to_string()])
            .await
            .unwrap();
        let score = ai_cache::cosine_similarity(&vectors[0], &vectors[1]).unwrap();
        assert!((score - 1.0).abs() < 1e-5, "{score}");
    }

    #[tokio::test]
    async fn empty_text_embeds_to_zero_vector() {
        let embeddings = StatisticalEmbeddings::defaults();
        let vectors = embeddings.embed(&["".to_string()]).await.unwrap();
        let norm: f32 = vectors[0].iter().map(|v| v * v).sum();
        assert_eq!(norm, 0.0);
    }

    #[tokio::test]
    async fn dimensions_are_normalized_to_power_of_two() {
        let embeddings = StatisticalEmbeddings::new(StatisticalConfig {
            dimensions: 100,
            ..Default::default()
        });
        let vectors = embeddings.embed(&["test".to_string()]).await.unwrap();
        assert_eq!(vectors[0].len(), 128, "rounded up to the next power of two");
    }
}
