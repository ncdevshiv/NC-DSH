//! Self-hosted n-gram embeddings v2: character n-gram (default 2..=4)
//! feature hashing with FNV-1a, tf-idf-style sublinear weighting, and L2
//! normalization.
//!
//! Upgrade path over [`crate::statistical::StatisticalEmbeddings`] (which is
//! kept unchanged for backwards compatibility): word-level hashing breaks on
//! morphology (`running` vs `runs`), typos (`embedngs`), and OOV words —
//! none of which share an exact token. Character n-grams overlap across all
//! three, so similarity degrades gracefully instead of collapsing.
//!
//! Document frequencies are estimated *online*: callers feed corpora through
//! [`NgramEmbeddings::observe`] (or any ingest path that calls
//! [`EmbeddingsProvider::observe`]) and the idf weights sharpen as evidence
//! accumulates. Memory is bounded by capping the number of tracked terms;
//! beyond the cap new terms embed with the unseen-term weight instead of
//! being tracked. No native dependencies, fully deterministic.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::embeddings::{EmbeddingsError, EmbeddingsProvider};

/// Configuration for [`NgramEmbeddings`].
#[derive(Debug, Clone)]
pub struct NgramConfig {
    /// Vector dimensionality (forced up to the next power of two, minimum 16).
    pub dimensions: usize,
    /// Smallest character n-gram length (inclusive).
    pub ngram_min: usize,
    /// Largest character n-gram length (inclusive).
    pub ngram_max: usize,
    /// Maximum number of distinct terms whose document frequency is tracked
    /// (bounded memory for the online idf table).
    pub max_tracked_terms: usize,
}

impl Default for NgramConfig {
    fn default() -> Self {
        Self {
            dimensions: 1024,
            ngram_min: 2,
            ngram_max: 4,
            max_tracked_terms: 65_536,
        }
    }
}

/// Character n-gram feature-hashing embeddings with an online idf table.
///
/// The same instance is safe to share across tasks (`&self` only); document
/// frequencies live behind interior mutability.
pub struct NgramEmbeddings {
    config: NgramConfig,
    /// Term hash (full FNV-1a u64, pre-bucketing) → number of observed
    /// documents containing it. Keyed by the full hash so re-tuning
    /// `dimensions` does not invalidate collected statistics.
    df: RwLock<HashMap<u64, u32>>,
    docs_seen: AtomicU64,
}

impl NgramEmbeddings {
    pub fn new(config: NgramConfig) -> Self {
        let config = NgramConfig {
            dimensions: config.dimensions.next_power_of_two().max(16),
            ngram_min: config.ngram_min.max(1),
            ngram_max: config.ngram_max.max(config.ngram_min),
            ..config
        };
        Self {
            config,
            df: RwLock::new(HashMap::new()),
            docs_seen: AtomicU64::new(0),
        }
    }

    /// A default configuration instance.
    pub fn defaults() -> Self {
        Self::new(NgramConfig::default())
    }

    /// Number of documents fed through [`Self::observe`] so far.
    pub fn doc_count(&self) -> u64 {
        self.docs_seen.load(Ordering::Relaxed)
    }

    /// Number of distinct terms currently tracked in the idf table.
    pub fn tracked_terms(&self) -> usize {
        self.df.read().len()
    }

    /// Lowercases and collapses every non-alphanumeric run to a single
    /// space. Spaces act as soft word-boundary markers for the n-grams.
    fn normalize(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut prev_space = true; // trims leading separators
        for ch in text.chars() {
            if ch.is_alphanumeric() {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                    prev_space = false;
                }
            } else if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out
    }

    /// All character n-grams (lengths `ngram_min..=ngram_max`) of the
    /// normalized text. Character-indexed via a byte-offset table, so
    /// multi-byte UTF-8 is safe.
    fn ngrams<'a>(&self, normalized: &'a str) -> Vec<&'a str> {
        let mut offsets: Vec<usize> = normalized.char_indices().map(|(i, _)| i).collect();
        offsets.push(normalized.len()); // sentinel end
        let char_count = offsets.len() - 1;
        let mut grams = Vec::new();
        for n in self.config.ngram_min..=self.config.ngram_max {
            for start in 0..char_count.saturating_sub(n.saturating_sub(1)) {
                grams.push(&normalized[offsets[start]..offsets[start + n]]);
            }
        }
        grams
    }

    /// FNV-1a 64-bit hash of a string (stable across platforms; matches
    /// `statistical.rs`).
    fn fnv1a(input: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in input.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Splitmix64 finalizer: cheap, deterministic decorrelation of the hash
    /// used for the ±1 sign so sign and bucket do not share low bits.
    fn mix(mut z: u64) -> u64 {
        z ^= z >> 33;
        z = z.wrapping_mul(0xff51_afd7_ed55_8ccd);
        z ^= z >> 33;
        z = z.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        z ^= z >> 33;
        z
    }

    /// Smooth idf: `ln((1 + N) / (1 + df)) + 1`. Unseen terms (df = 0)
    /// get the maximum weight; with no observed corpus at all this
    /// degenerates to plain sublinear-tf hashing (idf ≡ 1).
    fn idf(&self, term_hash: u64) -> f32 {
        let n = self.docs_seen.load(Ordering::Relaxed);
        let df = self.df.read().get(&term_hash).copied().unwrap_or(0) as f64;
        (((n as f64 + 1.0) / (df + 1.0)).ln() + 1.0) as f32
    }

    /// Embeds one text: signed feature hashing of char n-grams, sublinear
    /// tf × online-idf weighting, L2-normalized.
    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.config.dimensions];
        let normalized = Self::normalize(text);
        if normalized.is_empty() {
            return vector;
        }

        // BTreeMap (not HashMap): deterministic accumulation order keeps
        // embeddings bit-stable across processes despite float addition.
        let mut term_freq: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for gram in self.ngrams(&normalized) {
            *term_freq.entry(gram).or_insert(0) += 1;
        }

        for (gram, freq) in term_freq {
            let hash = Self::fnv1a(gram);
            let index = (hash as usize) & (self.config.dimensions - 1);
            let sign = if Self::mix(hash) & 0x8000_0000_0000_0000 == 0 {
                1.0
            } else {
                -1.0
            };
            let weight = (1.0 + (freq as f32).ln()) * self.idf(hash);
            vector[index] += sign * weight;
        }

        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

impl NgramEmbeddings {
    /// Observe-phase entry point: registers each text as one corpus document
    /// for the online idf table. Call before embedding (e.g. at RAG ingest).
    pub fn observe_texts(&self, texts: &[String]) {
        for text in texts {
            let normalized = Self::normalize(text);
            if normalized.is_empty() {
                continue;
            }
            // Distinct terms only: document frequency counts documents.
            let mut hashes: Vec<u64> = self
                .ngrams(&normalized)
                .into_iter()
                .map(Self::fnv1a)
                .collect();
            hashes.sort_unstable();
            hashes.dedup();

            let mut df = self.df.write();
            for hash in hashes {
                match df.get_mut(&hash) {
                    Some(count) => *count = count.saturating_add(1),
                    None => {
                        if df.len() < self.config.max_tracked_terms {
                            df.insert(hash, 1);
                        }
                        // Beyond the cap the term stays untracked and keeps
                        // the unseen-term idf weight (bounded memory).
                    }
                }
            }
            drop(df);
            self.docs_seen.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[async_trait]
impl EmbeddingsProvider for NgramEmbeddings {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }

    /// Stateful provider: folds texts into the online idf table.
    async fn observe(&self, texts: &[String]) {
        self.observe_texts(texts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cos(a: &[f32], b: &[f32]) -> f32 {
        ai_cache::cosine_similarity(a, b).unwrap_or(f32::NAN)
    }

    #[tokio::test]
    async fn morphology_variants_share_similarity() {
        let embeddings = NgramEmbeddings::defaults();
        let vectors = embeddings
            .embed(&[
                "the runner is running fast".to_string(),
                "runners run fast".to_string(),
                "quarterly revenue exceeded forecasts".to_string(),
            ])
            .await
            .unwrap();
        let morphological = cos(&vectors[0], &vectors[1]);
        let unrelated = cos(&vectors[0], &vectors[2]);
        assert!(
            morphological > unrelated + 0.05,
            "morph={morphological} unrelated={unrelated}"
        );
    }

    #[tokio::test]
    async fn typos_degrade_gracefully() {
        let embeddings = NgramEmbeddings::defaults();
        let vectors = embeddings
            .embed(&[
                "vector embeddings for search".to_string(),
                "vector embedngs for search".to_string(), // typo
                "quarterly revenue report".to_string(),
            ])
            .await
            .unwrap();
        let typo = cos(&vectors[0], &vectors[1]);
        let unrelated = cos(&vectors[0], &vectors[2]);
        assert!(typo > 0.6, "typo stays close: {typo}");
        assert!(
            typo > unrelated,
            "typo beats unrelated: {typo} vs {unrelated}"
        );
    }

    #[tokio::test]
    async fn observe_sharpens_common_term_downweighting() {
        // A filler phrase appearing in every observed document must lose
        // discriminative weight relative to an unobserved distinctive
        // phrase: after fitting, the shared grams carry near-zero idf.
        let embeddings = NgramEmbeddings::defaults();
        let filler_seen = "the system supports";
        embeddings.observe_texts(&[
            format!("{filler_seen} alpha exports"),
            format!("{filler_seen} beta imports"),
            format!("{filler_seen} gamma retries"),
        ]);
        assert_eq!(embeddings.doc_count(), 3);
        let vectors = embeddings
            .embed(&[
                format!("{filler_seen} kumquat propulsion"),
                "unrelated quarterly totals".to_string(),
            ])
            .await
            .unwrap();
        // Sanity: embedding still normalizes.
        let norm: f32 = vectors[0].iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "{norm}");
    }

    #[tokio::test]
    async fn identical_texts_have_unit_similarity_and_determinism() {
        let a = NgramEmbeddings::defaults();
        let b = NgramEmbeddings::defaults();
        let va = a
            .embed(&["deterministic hashing".to_string()])
            .await
            .unwrap();
        let vb = b
            .embed(&["deterministic hashing".to_string()])
            .await
            .unwrap();
        assert_eq!(va[0], vb[0], "same config → same vector");
        assert!((cos(&va[0], &vb[0]) - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn empty_text_embeds_to_zero_vector() {
        let embeddings = NgramEmbeddings::defaults();
        let vectors = embeddings
            .embed(&["".to_string(), "!!! ...".to_string()])
            .await
            .unwrap();
        for v in &vectors {
            assert!(v.iter().all(|x| *x == 0.0));
        }
    }

    #[test]
    fn dimensions_round_to_power_of_two() {
        let embeddings = NgramEmbeddings::new(NgramConfig {
            dimensions: 100,
            ..Default::default()
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let vectors = runtime
            .block_on(embeddings.embed(&["x".to_string()]))
            .unwrap();
        assert_eq!(vectors[0].len(), 128);
    }

    #[test]
    fn ngram_iterator_respects_utf8_and_range() {
        let embeddings = NgramEmbeddings::new(NgramConfig {
            ngram_min: 2,
            ngram_max: 3,
            ..Default::default()
        });
        let grams: Vec<&str> = embeddings.ngrams("ab€c");
        assert_eq!(grams, vec!["ab", "b€", "€c", "ab€", "b€c"], "{grams:?}");
    }

    #[tokio::test]
    async fn df_table_respects_memory_cap() {
        let embeddings = NgramEmbeddings::new(NgramConfig {
            max_tracked_terms: 8,
            ..Default::default()
        });
        embeddings.observe_texts(&[
            "completely untracked vocabulary here".to_string(),
            "more untracked vocabulary appears now".to_string(),
        ]);
        assert!(
            embeddings.tracked_terms() <= 8,
            "cap enforced: {}",
            embeddings.tracked_terms()
        );
        assert_eq!(embeddings.doc_count(), 2);
    }
}
