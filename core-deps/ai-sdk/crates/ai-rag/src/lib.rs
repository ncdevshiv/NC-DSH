//! RAG (PRD §3.8): chunking, ingestion, vector retrieval, hybrid
//! keyword+semantic search, reranking, and context assembly. Storage and
//! embeddings are real (`ai-storage` vector store, `ai-memory` embeddings
//! provider). Keyword scoring is true BM25 with corpus idf when statistics
//! have been accumulated at ingest (falling back to constant-idf scoring
//! otherwise), and fusion offers weighted-alpha blending plus genuine
//! reciprocal rank fusion (`HybridStrategy::ReciprocalRank`).

mod chunking;
mod hybrid;
mod pipeline;

pub use chunking::{ChunkingStrategy, chunk_document};
pub use hybrid::{
    CorpusStats, HybridStrategy, RRF_K, bm25_corpus, bm25_score, cosine, hybrid_fusion,
    hybrid_fusion_with, keyword_search, keyword_search_corpus, reciprocal_rank_fusion,
};
pub use pipeline::{ContextAssembler, RagConfig, RagPipeline, RetrievedChunk};

use ai_errors::AiError;

/// A reranker: reorders retrieved chunks (e.g. cross-encoder or LLM-based).
#[async_trait::async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        candidates: Vec<RetrievedChunk>,
    ) -> Result<Vec<RetrievedChunk>, AiError>;
}

/// A no-op reranker that preserves the input order (used when no external
/// reranker is configured).
pub struct IdentityReranker;

#[async_trait::async_trait]
impl Reranker for IdentityReranker {
    async fn rerank(
        &self,
        _query: &str,
        candidates: Vec<RetrievedChunk>,
    ) -> Result<Vec<RetrievedChunk>, AiError> {
        Ok(candidates)
    }
}

/// A reranker that re-scores candidates by BM25 keyword overlap with the
/// query (real, deterministic, no external service).
pub struct KeywordReranker {
    /// Blend factor: 0 = keep original scores, 1 = keyword scores only.
    pub blend: f32,
}

impl Default for KeywordReranker {
    fn default() -> Self {
        Self { blend: 0.5 }
    }
}

#[async_trait::async_trait]
impl Reranker for KeywordReranker {
    async fn rerank(
        &self,
        query: &str,
        candidates: Vec<RetrievedChunk>,
    ) -> Result<Vec<RetrievedChunk>, AiError> {
        let keyword_scores = keyword_search(query, &candidates, 1.2, 0.75);
        let mut scored: Vec<(RetrievedChunk, f32)> = candidates
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| {
                let kw = keyword_scores.get(&index).copied().unwrap_or(0.0);
                let blended = self.blend * kw + (1.0 - self.blend) * chunk.score;
                (chunk, blended)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .map(|(chunk, score)| RetrievedChunk { score, ..chunk })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_reranker_preserves_order() {
        let reranker = IdentityReranker;
        let chunks = vec![
            RetrievedChunk {
                id: "a".into(),
                text: "x".into(),
                score: 0.9,
                metadata: serde_json::Value::Null,
            },
            RetrievedChunk {
                id: "b".into(),
                text: "y".into(),
                score: 0.1,
                metadata: serde_json::Value::Null,
            },
        ];
        let out = futures::executor::block_on(reranker.rerank("q", chunks)).unwrap();
        assert_eq!(out[0].id, "a");
    }
}

#[cfg(test)]
mod proptests;
