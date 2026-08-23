//! Hybrid retrieval: BM25 keyword scoring (corpus-aware or standalone)
//! combined with semantic cosine similarity, fused by weighted-alpha
//! blending or true reciprocal rank fusion (RRF).

use std::collections::HashMap;

use crate::RetrievedChunk;

/// Tokenizes text into lowercase word stems.
///
/// Lowercasing happens FIRST, then splitting on non-token characters:
/// case folding can expand one character into several (e.g. 'İ' U+0130
/// lowercases to "i" + U+0307 COMBINING DOT ABOVE, and the combining mark is
/// not alphanumeric), and some cased characters are left untouched by
/// `to_lowercase` (e.g. '𝓐' U+1D4D0) while still being uppercase. Splitting
/// on `!is_alphanumeric() || is_uppercase` therefore guarantees, by
/// construction, that every token is a clean run of non-uppercase
/// alphanumeric characters; ASCII behavior is unchanged.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() || c.is_uppercase())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// BM25 keyword score of `query` against a document **without corpus
/// statistics**.
///
/// `k1` controls term-frequency saturation (typical 1.2), `b` controls
/// document-length normalization (typical 0.75). With no corpus statistics
/// available, idf is a small constant (1.0) and the document length is used
/// as-is rather than relative to a corpus average — so this function cannot
/// downweight common terms or normalize length against real data. Prefer
/// [`bm25_corpus`] whenever ingest has produced a [`CorpusStats`].
pub fn bm25_score(query: &str, document: &str, k1: f32, b: f32) -> f32 {
    let query_terms = tokenize(query);
    let doc_terms = tokenize(document);
    if doc_terms.is_empty() || query_terms.is_empty() {
        return 0.0;
    }
    let doc_len = doc_terms.len() as f32;
    let mut term_freq: HashMap<String, usize> = HashMap::new();
    for term in &doc_terms {
        *term_freq.entry(term.clone()).or_insert(0) += 1;
    }

    let mut score = 0.0f32;
    for term in &query_terms {
        let tf = *term_freq.get(term).unwrap_or(&0) as f32;
        if tf == 0.0 {
            continue;
        }
        // No corpus statistics: idf is a constant so present terms
        // contribute; see [`bm25_corpus`] for real idf.
        let idf = 1.0f32;
        let normalization = 1.0 - b + b * (doc_len / 1.0);
        score += idf * (tf * (k1 + 1.0)) / (tf + k1 * normalization);
    }
    score
}

/// Document-frequency statistics over an ingested corpus, feeding
/// [`bm25_corpus`]. Built online by [`crate::RagPipeline`] at ingest time.
#[derive(Debug, Clone, Default)]
pub struct CorpusStats {
    /// Number of observed documents (N).
    doc_count: u64,
    /// Total token count across all observed documents.
    total_len: u64,
    /// term → number of documents containing it (n).
    doc_freq: HashMap<String, u64>,
}

impl CorpusStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one document into the statistics.
    pub fn observe(&mut self, document: &str) {
        let terms = tokenize(document);
        self.doc_count += 1;
        self.total_len += terms.len() as u64;
        for term in terms.into_iter().collect::<std::collections::BTreeSet<_>>() {
            *self.doc_freq.entry(term).or_insert(0) += 1;
        }
    }

    /// Statistics over a batch of documents, in order.
    pub fn from_texts(texts: &[String]) -> Self {
        let mut stats = Self::new();
        for text in texts {
            stats.observe(text);
        }
        stats
    }

    /// Observed document count N.
    pub fn doc_count(&self) -> u64 {
        self.doc_count
    }

    /// Average token length of observed documents (0.0 when empty).
    pub fn average_doc_len(&self) -> f32 {
        if self.doc_count == 0 {
            0.0
        } else {
            self.total_len as f32 / self.doc_count as f32
        }
    }

    /// Number of observed documents containing `term`.
    pub fn doc_freq(&self, term: &str) -> u64 {
        self.doc_freq.get(term).copied().unwrap_or(0)
    }

    /// True when no documents have been observed.
    pub fn is_empty(&self) -> bool {
        self.doc_count == 0
    }
}

/// BM25 score with **real corpus idf**:
/// `idf = ln((N - n + 0.5)/(n + 0.5) + 1)` where N is the number of
/// observed documents and n the number containing the term; length
/// normalization uses the true corpus average instead of a constant 1.0.
///
/// Falls back to constant-idf behavior when `stats` is empty (N = 0), which
/// matches [`bm25_score`].
pub fn bm25_corpus(query: &str, document: &str, stats: &CorpusStats, k1: f32, b: f32) -> f32 {
    let query_terms = tokenize(query);
    let doc_terms = tokenize(document);
    if doc_terms.is_empty() || query_terms.is_empty() || stats.is_empty() {
        return 0.0;
    }
    let n_docs = stats.doc_count as f32;
    let avgdl = stats.average_doc_len().max(1e-6);
    let doc_len = doc_terms.len() as f32;
    let mut term_freq: HashMap<String, usize> = HashMap::new();
    for term in &doc_terms {
        *term_freq.entry(term.clone()).or_insert(0) += 1;
    }

    let mut score = 0.0f32;
    for term in &query_terms {
        let tf = *term_freq.get(term).unwrap_or(&0) as f32;
        if tf == 0.0 {
            continue;
        }
        let df = stats.doc_freq(term) as f32;
        // Robertson/Sparck-Jones idf with the +1 inside the log keeping it
        // non-negative for terms present in every document.
        let idf = (((n_docs - df + 0.5) / (df + 0.5)) + 1.0).ln();
        let normalization = 1.0 - b + b * (doc_len / avgdl);
        score += idf * (tf * (k1 + 1.0)) / (tf + k1 * normalization);
    }
    score
}

/// Cosine similarity between two vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return None;
    }
    let val = dot / (na.sqrt() * nb.sqrt());
    Some(val.clamp(-1.0, 1.0))
}

/// Ranks candidate chunks by BM25 keyword overlap with `query`.
/// Returns a map from candidate index → keyword score.
pub fn keyword_search(
    query: &str,
    candidates: &[RetrievedChunk],
    k1: f32,
    b: f32,
) -> HashMap<usize, f32> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, chunk)| (index, bm25_score(query, &chunk.text, k1, b)))
        .filter(|(_, score)| *score > 0.0)
        .collect()
}

/// Corpus-aware variant of [`keyword_search`] using [`bm25_corpus`].
pub fn keyword_search_corpus(
    query: &str,
    candidates: &[RetrievedChunk],
    stats: &CorpusStats,
    k1: f32,
    b: f32,
) -> HashMap<usize, f32> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, chunk)| (index, bm25_corpus(query, &chunk.text, stats, k1, b)))
        .filter(|(_, score)| *score > 0.0)
        .collect()
}

/// RRF damping constant from Cormack, Clarke & Buettcher (2009); rank r in
/// a list contributes `1 / (RRF_K + r)` with ranks starting at 1.
pub const RRF_K: f32 = 60.0;

/// True reciprocal rank fusion over ranked id lists.
///
/// Each list contributes `1/(k + rank)` to every id it contains (rank 1 =
/// first). The result is sorted by fused score descending with ids as a
/// deterministic tie-break; every id appearing in any list is present.
pub fn reciprocal_rank_fusion(rankings: &[Vec<String>], k: f32) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for ranking in rankings {
        for (position, id) in ranking.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + position as f32 + 1.0);
        }
    }
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    ranked
}

/// How semantic and keyword signals are combined.
///
/// - [`HybridStrategy::WeightedAlpha`] (default): historical behavior —
///   `alpha * semantic_score + (1 - alpha) * keyword_score`, both on their
///   raw scales (cosine ≈ [-1,1], BM25 unbounded).
/// - [`HybridStrategy::ReciprocalRank`]: true RRF — each stage's ranking
///   contributes `1/(RRF_K + rank)`; scale-free and robust to score-scale
///   mismatch between stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HybridStrategy {
    #[default]
    WeightedAlpha,
    ReciprocalRank,
}

/// Fusion dispatch for [`HybridStrategy`]. See [`hybrid_fusion_with`].
pub fn hybrid_fusion(
    semantic: &[(String, f32)],
    keyword: &HashMap<String, f32>,
    alpha: f32,
) -> Vec<(String, f32)> {
    weighted_alpha_fusion(semantic, keyword, alpha)
}

/// Combines semantic and keyword results per `strategy`.
///
/// For [`HybridStrategy::ReciprocalRank`], rankings are derived internally:
/// `semantic` is sorted by score descending (stable), and `keyword` entries
/// likewise; ties inside a list keep input order. `alpha` is ignored for RRF.
pub fn hybrid_fusion_with(
    strategy: HybridStrategy,
    semantic: &[(String, f32)],
    keyword: &HashMap<String, f32>,
    alpha: f32,
) -> Vec<(String, f32)> {
    match strategy {
        HybridStrategy::WeightedAlpha => weighted_alpha_fusion(semantic, keyword, alpha),
        HybridStrategy::ReciprocalRank => {
            let mut semantic_ranked = semantic.to_vec();
            semantic_ranked.sort_by(deterministic_score_order);
            let mut keyword_ranked: Vec<(String, f32)> = keyword
                .iter()
                .map(|(id, score)| (id.clone(), *score))
                .collect();
            keyword_ranked.sort_by(deterministic_score_order);
            reciprocal_rank_fusion(
                &[
                    semantic_ranked
                        .into_iter()
                        .map(|(id, _)| id)
                        .collect::<Vec<_>>(),
                    keyword_ranked.into_iter().map(|(id, _)| id).collect(),
                ],
                RRF_K,
            )
        }
    }
}

/// Weighted-alpha blend (the historical fusion): sums
/// `alpha * semantic + (1 - alpha) * keyword` over the union of ids and
/// sorts descending.
fn weighted_alpha_fusion(
    semantic: &[(String, f32)],
    keyword: &HashMap<String, f32>,
    alpha: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for (id, score) in semantic {
        *scores.entry(id.clone()).or_insert(0.0) += alpha * score;
    }
    for (id, score) in keyword {
        *scores.entry(id.clone()).or_insert(0.0) += (1.0 - alpha) * score;
    }
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(deterministic_score_order);
    ranked
}

/// Score-descending comparator with an id tie-break: equal scores must not
/// depend on `HashMap` iteration order (random per process), or recall
/// boundaries flip between runs.
fn deterministic_score_order(a: &(String, f32), b: &(String, f32)) -> std::cmp::Ordering {
    b.1.partial_cmp(&a.1)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.0.cmp(&b.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_and_lowercases() {
        assert_eq!(tokenize("Hello, World! 42"), vec!["hello", "world", "42"]);
        assert!(tokenize("   ").is_empty());
    }

    /// Regression: 'İ' (U+0130) lowercases to "i" + U+0307 COMBINING DOT
    /// ABOVE; the combining mark must become a token separator, never leak
    /// into a token.
    #[test]
    fn tokenize_reseparates_after_unicode_case_expansion() {
        let tokens = tokenize("İstanbul");
        assert_eq!(tokens, vec!["i", "stanbul"], "{tokens:?}");
        for token in &tokens {
            assert!(!token.is_empty());
            assert!(
                token
                    .chars()
                    .all(|c| c.is_alphanumeric() && !c.is_uppercase()),
                "dirty token {token:?}"
            );
        }
        // ASCII behavior is unchanged.
        assert_eq!(tokenize("HELLO, World"), vec!["hello", "world"]);
        // Cased characters left untouched by to_lowercase (math
        // alphanumerics) must act as separators, never token contents.
        assert_eq!(tokenize("a𝓐b"), vec!["a", "b"]);
    }

    #[test]
    fn bm25_ranks_relevant_documents_higher() {
        let relevant = "The quick brown fox jumps over the lazy dog. Foxes are quick.";
        let irrelevant = "The stock market opened flat on Tuesday morning.";
        let q = "quick fox";
        let score_relevant = bm25_score(q, relevant, 1.2, 0.75);
        let score_irrelevant = bm25_score(q, irrelevant, 1.2, 0.75);
        assert!(
            score_relevant > score_irrelevant,
            "{score_relevant} vs {score_irrelevant}"
        );
        assert!(score_irrelevant == 0.0);
    }

    #[test]
    fn cosine_identical_and_orthogonal() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]).unwrap() - 1.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0]).unwrap() - 0.0).abs() < 1e-6);
        assert!(cosine(&[1.0], &[1.0, 2.0]).is_none());
    }

    #[test]
    fn keyword_search_scores_only_matching() {
        let candidates = vec![
            RetrievedChunk {
                id: "a".into(),
                text: "rust is a systems language".into(),
                score: 0.0,
                metadata: serde_json::Value::Null,
            },
            RetrievedChunk {
                id: "b".into(),
                text: "python is a scripting language".into(),
                score: 0.0,
                metadata: serde_json::Value::Null,
            },
        ];
        let scores = keyword_search("rust", &candidates, 1.2, 0.75);
        assert!(scores.contains_key(&0));
        assert!(!scores.contains_key(&1));
    }

    #[test]
    fn hybrid_fusion_combines_scores() {
        let semantic = vec![("a".to_string(), 0.8), ("b".to_string(), 0.2)];
        let keyword: HashMap<String, f32> =
            HashMap::from([("a".to_string(), 2.0), ("c".to_string(), 1.0)]);
        let ranked = hybrid_fusion(&semantic, &keyword, 0.5);
        assert_eq!(ranked[0].0, "a", "present in both ranks first");
        assert!(ranked.iter().any(|(id, _)| id == "c"));
    }

    /// Known-answer RRF test with hand-computed scores.
    ///
    /// Lists A=[a,b,c] and B=[b,c,d], k=60:
    ///   a: 1/61            = 0.0163934…
    ///   b: 1/62 + 1/61     = 0.0325225…
    ///   c: 1/63 + 1/62     = 0.0320020…
    ///   d: 1/63            = 0.0158730…
    /// so the fused order is b > c > a > d.
    #[test]
    fn reciprocal_rank_fusion_known_answer() {
        let list_a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let list_b = vec!["b".to_string(), "c".to_string(), "d".to_string()];
        let fused = reciprocal_rank_fusion(&[list_a, list_b], RRF_K);
        let ids: Vec<&str> = fused.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "a", "d"], "{fused:?}");

        let expected: Vec<(&str, f64)> = vec![
            ("b", 1.0 / 62.0 + 1.0 / 61.0),
            ("c", 1.0 / 63.0 + 1.0 / 62.0),
            ("a", 1.0 / 61.0),
            ("d", 1.0 / 63.0),
        ];
        for ((id, score), (want_id, want_score)) in fused.iter().zip(expected) {
            assert_eq!(id, want_id);
            assert!(
                (*score as f64 - want_score).abs() < 1e-6,
                "{id}: {score} vs {want_score}"
            );
        }
        // Every id from every list is present exactly once.
        assert_eq!(fused.len(), 4);
    }

    #[test]
    fn rrf_fusion_ignores_alpha_and_preserves_default_behavior() {
        let semantic = vec![("a".to_string(), 0.9), ("b".to_string(), 0.4)];
        let keyword: HashMap<String, f32> =
            HashMap::from([("a".to_string(), 3.0), ("c".to_string(), 1.5)]);

        // Default strategy == historical weighted-alpha path.
        let legacy = hybrid_fusion(&semantic, &keyword, 0.7);
        let default_strategy =
            hybrid_fusion_with(HybridStrategy::default(), &semantic, &keyword, 0.7);
        assert_eq!(legacy, default_strategy);

        // WeightedAlpha explicit == legacy too.
        let weighted = hybrid_fusion_with(HybridStrategy::WeightedAlpha, &semantic, &keyword, 0.7);
        assert_eq!(weighted, legacy);

        // RRF runs and ranks `a` first (rank-1 in both lists).
        let rrf = hybrid_fusion_with(HybridStrategy::ReciprocalRank, &semantic, &keyword, 0.7);
        assert_eq!(rrf[0].0, "a");
        // RRF scores are bounded by the number of lists (≤ 2/61 here),
        // unlike weighted sums — scale-free by construction.
        for (_, score) in &rrf {
            assert!(*score <= 2.0 / (RRF_K + 1.0) + 1e-6, "{score}");
        }
    }

    fn corpus_fixture() -> CorpusStats {
        CorpusStats::from_texts(&[
            // "postgres" in 3 of 5 docs (incl. the stuffed one); "indexing"
            // in exactly one. avgdl = (3+5+4+4+5)/5 = 4.2.
            "postgres postgres postgres".to_string(),
            "indexing strategies explained simply today".to_string(),
            "postgres database backup procedures".to_string(),
            "postgres migration runbook draft".to_string(),
            "quarterly revenue report published online".to_string(),
        ])
    }

    /// Hand-check of the idf formula on the fixture corpus:
    /// N=5; "indexing" n=1 → ln((5-1+0.5)/(1+0.5)+1) = ln(4) ≈ 1.38629;
    ///       "postgres" n=3 → ln((5-3+0.5)/(3+0.5)+1) = ln(12/7) ≈ 0.53900.
    #[test]
    fn bm25_corpus_idf_matches_formula() {
        let stats = corpus_fixture();
        assert_eq!(stats.doc_count(), 5);
        assert!((stats.average_doc_len() - 21.0 / 5.0).abs() < 1e-5);
        assert_eq!(stats.doc_freq("postgres"), 3);
        let idf_indexing = (((5.0f32 - 1.0 + 0.5) / (1.0 + 0.5)) + 1.0).ln();
        let idf_postgres = (((5.0f32 - 3.0 + 0.5) / (3.0 + 0.5)) + 1.0).ln();
        assert!(
            (stats_bm25_idf(&stats, "indexing") - idf_indexing).abs() < 1e-6,
            "{} vs {idf_indexing}",
            stats_bm25_idf(&stats, "indexing")
        );
        assert!((stats_bm25_idf(&stats, "postgres") - idf_postgres).abs() < 1e-6);
    }

    // Helper exposing the idf used inside bm25_corpus (same formula).
    fn stats_bm25_idf(stats: &CorpusStats, term: &str) -> f32 {
        let n_docs = stats.doc_count() as f32;
        let df = stats.doc_freq(term) as f32;
        (((n_docs - df + 0.5) / (df + 0.5)) + 1.0).ln()
    }

    /// Discriminating fixture: constant-idf BM25 (`bm25_score`) ranks a
    /// short document stuffed with a corpus-common term ("postgres") above
    /// the genuinely relevant document; corpus idf corrects this because
    /// rare "indexing" outweighs ubiquitous "postgres".
    #[test]
    fn bm25_corpus_beats_constant_idf_on_discriminating_fixture() {
        let stats = corpus_fixture();
        let query = "postgres indexing";
        let common_stuffed = "postgres postgres postgres";
        let actually_relevant = "indexing strategies explained simply today";

        let old_common = bm25_score(query, common_stuffed, 1.2, 0.75);
        let old_relevant = bm25_score(query, actually_relevant, 1.2, 0.75);
        assert!(
            old_common > old_relevant,
            "precondition: constant-idf misranks ({old_common} <= {old_relevant})"
        );

        let new_common = bm25_corpus(query, common_stuffed, &stats, 1.2, 0.75);
        let new_relevant = bm25_corpus(query, actually_relevant, &stats, 1.2, 0.75);
        assert!(
            new_relevant > new_common,
            "corpus idf must rank the relevant doc first: {new_relevant} vs {new_common}"
        );
        println!("constant-idf: stuffed={old_common:.4} relevant={old_relevant:.4}");
        println!("corpus-idf:   stuffed={new_common:.4} relevant={new_relevant:.4}");
    }

    #[test]
    fn bm25_corpus_empty_stats_scores_zero() {
        let stats = CorpusStats::new();
        assert!(stats.is_empty());
        assert_eq!(bm25_corpus("any query", "some doc", &stats, 1.2, 0.75), 0.0);
    }
}
