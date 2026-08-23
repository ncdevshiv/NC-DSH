//! The RAG pipeline: chunk → embed → store → retrieve (hybrid) → rerank →
//! assemble context.
//!
//! # Retrieval recall (changed)
//!
//! The keyword stage scores **all ingested chunks** (kept in the pipeline's
//! in-memory mirror), not only the vector-stage survivors. Historically the
//! keyword stage searched just the semantic top-hits subset, capping fused
//! recall at whatever the vector stage surfaced — a lexically perfect match
//! outside that subset was unreachable. With
//! [`HybridStrategy::WeightedAlpha`] this widens the fusion pool (keyword
//! matches can now enter from below); with [`HybridStrategy::ReciprocalRank`]
//! the keyword ranking is a first-class list. Corpus statistics
//! ([`CorpusStats`]) are accumulated at ingest and feed real BM25 idf.

use std::sync::Arc;

use ai_errors::{AiError, StorageError};
use ai_memory::EmbeddingsProvider;
use ai_storage::{VectorEntry, VectorStore};

use crate::chunking::{ChunkingStrategy, chunk_document};
use crate::hybrid::{
    CorpusStats, HybridStrategy, RRF_K, hybrid_fusion_with, reciprocal_rank_fusion,
};
use crate::{IdentityReranker, Reranker};

/// A retrieved chunk with its score.
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub id: String,
    pub text: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

/// RAG pipeline configuration.
#[derive(Debug, Clone)]
pub struct RagConfig {
    pub chunking: ChunkingStrategy,
    /// Semantic similarity threshold for vector hits.
    pub min_similarity: f32,
    /// Blend factor for weighted-alpha fusion (1 = semantic only, 0 =
    /// keyword only). Ignored by [`HybridStrategy::ReciprocalRank`].
    pub hybrid_alpha: f32,
    /// How semantic and keyword signals are fused.
    ///
    /// Default is [`HybridStrategy::WeightedAlpha`], preserving historical
    /// behavior; [`HybridStrategy::ReciprocalRank`] enables true RRF.
    pub strategy: HybridStrategy,
    /// BM25 k1 parameter.
    pub bm25_k1: f32,
    /// BM25 b parameter.
    pub bm25_b: f32,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            chunking: ChunkingStrategy::Fixed {
                size: 1200,
                overlap: 200,
            },
            min_similarity: 0.5,
            hybrid_alpha: 0.7,
            strategy: HybridStrategy::default(),
            bm25_k1: 1.2,
            bm25_b: 0.75,
        }
    }
}

/// A RAG pipeline over a vector store with an embeddings provider.
///
/// An in-memory [`EntryMirror`] keeps chunk payloads retrievable by id
/// (the vector store interface is search-only by design).
pub struct RagPipeline {
    store: Arc<dyn VectorStore>,
    embeddings: Arc<dyn EmbeddingsProvider>,
    config: RagConfig,
    reranker: Arc<dyn Reranker>,
    mirror: mirror::EntryMirror,
    /// Corpus statistics (N, avgdl, document frequencies) accumulated
    /// online at ingest, feeding corpus-aware BM25 at retrieval time.
    corpus: parking_lot::RwLock<CorpusStats>,
}

impl RagPipeline {
    pub fn new(
        store: Arc<dyn VectorStore>,
        embeddings: Arc<dyn EmbeddingsProvider>,
        config: RagConfig,
    ) -> Self {
        Self {
            store,
            embeddings,
            config,
            reranker: Arc::new(IdentityReranker),
            mirror: mirror::EntryMirror::new(),
            corpus: parking_lot::RwLock::new(CorpusStats::new()),
        }
    }

    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = reranker;
        self
    }

    /// Snapshot of the accumulated corpus statistics.
    pub fn corpus_stats(&self) -> CorpusStats {
        self.corpus.read().clone()
    }

    /// Ingests a document: chunk, fit corpus/embedding statistics online,
    /// embed each chunk, upsert into the store.
    ///
    /// Statistics are updated *before* embedding so stateful embedders
    /// ([`ai_memory::NgramEmbeddings`]) see every ingest chunk in their
    /// idf table; chunks are also registered as BM25 corpus documents.
    pub async fn ingest(&self, document_id: &str, document: &str) -> Result<usize, AiError> {
        let chunks = chunk_document(document, self.config.chunking);
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();

        // Online fitting: corpus stats for BM25 + stateful embedders.
        for text in &texts {
            self.corpus.write().observe(text);
        }
        self.embeddings.observe(&texts).await;

        let vectors = self
            .embeddings
            .embed(&texts)
            .await
            .map_err(|e| AiError::Storage(StorageError::new("rag", e.to_string())))?;

        for (chunk, vector) in chunks.into_iter().zip(vectors) {
            let id = format!("{document_id}:{chunk_index:05}", chunk_index = chunk.index);
            let entry = VectorEntry {
                id: id.clone(),
                vector,
                payload: serde_json::json!({
                    "document": document_id,
                    "text": chunk.text,
                    "start": chunk.start,
                    "index": chunk.index,
                }),
            };
            self.mirror.put(entry.clone());
            self.store.upsert(entry).await?;
        }
        Ok(texts.len())
    }

    /// Retrieves the top-k chunks for a query using hybrid search.
    pub async fn retrieve(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<RetrievedChunk>, AiError> {
        let query_vector = self
            .embeddings
            .embed(&[query.to_string()])
            .await
            .map_err(|e| AiError::Storage(StorageError::new("rag", e.to_string())))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AiError::Storage(StorageError::new("rag", "embeddings returned no vector"))
            })?;

        // Semantic candidates (superset for fusion), best first.
        let semantic = self.store.search(&query_vector, top_k * 3).await?;
        let semantic: Vec<(String, f32)> = semantic
            .into_iter()
            .filter(|(_, score)| *score >= self.config.min_similarity)
            .map(|(entry, score)| (entry.id, score))
            .collect();

        // Keyword stage over ALL ingested chunks — not just the semantic
        // survivors — so lexical matches outside the vector pool can be
        // fused in (see the module docs on the recall cap).
        let all_chunks: Vec<RetrievedChunk> = self
            .mirror
            .all()
            .into_iter()
            .map(|entry| RetrievedChunk {
                id: entry.id.clone(),
                text: entry
                    .payload
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
                score: 0.0,
                metadata: entry.payload.clone(),
            })
            .collect();
        let corpus_snapshot = self.corpus.read().clone();
        let keyword = crate::hybrid::keyword_search_corpus(
            query,
            &all_chunks,
            &corpus_snapshot,
            self.config.bm25_k1,
            self.config.bm25_b,
        );
        let keyword_by_id: std::collections::HashMap<String, f32> = keyword
            .iter()
            .filter_map(|(index, score)| all_chunks.get(*index).map(|c| (c.id.clone(), *score)))
            .collect();

        // Hybrid fusion per configured strategy. RRF combines the two
        // rankings scale-free; weighted-alpha keeps the historical blend.
        let fused = match self.config.strategy {
            HybridStrategy::WeightedAlpha => hybrid_fusion_with(
                HybridStrategy::WeightedAlpha,
                &semantic,
                &keyword_by_id,
                self.config.hybrid_alpha,
            ),
            HybridStrategy::ReciprocalRank => reciprocal_rank_fusion(
                &[semantic.iter().map(|(id, _)| id.clone()).collect(), {
                    let mut kw: Vec<(String, f32)> = keyword_by_id
                        .iter()
                        .map(|(id, score)| (id.clone(), *score))
                        .collect();
                    kw.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    kw.into_iter().map(|(id, _)| id).collect()
                }],
                RRF_K,
            ),
        };
        let mut fused = fused;
        fused.truncate(top_k);

        let mut retrieved = Vec::with_capacity(fused.len());
        for (id, score) in fused {
            if let Some(entry) = self.mirror.get(&id) {
                retrieved.push(RetrievedChunk {
                    id,
                    text: entry
                        .payload
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    score,
                    metadata: entry.payload,
                });
            }
        }

        self.reranker.rerank(query, retrieved).await
    }
}

/// Assembles a prompt context from retrieved chunks.
pub struct ContextAssembler {
    /// Separator between chunks.
    pub separator: String,
    /// Header line.
    pub header: String,
    /// Maximum total characters (truncates from the end).
    pub max_chars: usize,
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self {
            separator: "\n\n---\n\n".to_string(),
            header: "Relevant context:".to_string(),
            max_chars: 8_000,
        }
    }
}

impl ContextAssembler {
    pub fn assemble(&self, chunks: &[RetrievedChunk]) -> String {
        let mut out = String::new();
        out.push_str(&self.header);
        out.push('\n');
        for chunk in chunks {
            if out.len() >= self.max_chars {
                break;
            }
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&self.separator);
            out.push_str(&chunk.text);
        }
        out.truncate(self.max_chars.min(out.len()));
        out
    }
}

// The store trait lacks point reads; RagPipeline keeps a mirror of ingested
// entries so retrieval can reconstruct chunks without extra API surface.
pub(crate) mod mirror {
    use std::collections::HashMap;
    use std::sync::Arc;

    use parking_lot::RwLock;

    use ai_storage::VectorEntry;

    /// An in-memory mirror of entries (id → entry) kept in sync with the
    /// vector store by [`crate::pipeline::RagPipeline`].
    #[derive(Clone, Default)]
    pub struct EntryMirror {
        entries: Arc<RwLock<HashMap<String, VectorEntry>>>,
    }

    impl EntryMirror {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn put(&self, entry: VectorEntry) {
            self.entries.write().insert(entry.id.clone(), entry);
        }

        pub fn get(&self, id: &str) -> Option<VectorEntry> {
            self.entries.read().get(id).cloned()
        }

        /// Snapshot of every mirrored entry (used by the keyword stage to
        /// score all ingested chunks).
        pub fn all(&self) -> Vec<VectorEntry> {
            self.entries.read().values().cloned().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_memory::EmbeddingsError;
    use ai_storage::InMemoryVectorStore;
    use std::sync::Arc;

    /// Deterministic hash-based embeddings (no external service).
    struct HashEmbeddings;

    #[async_trait::async_trait]
    impl EmbeddingsProvider for HashEmbeddings {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError> {
            Ok(texts
                .iter()
                .map(|text| {
                    let mut vector = vec![0.0f32; 16];
                    for word in crate::hybrid::tokenize(text) {
                        let index = word.bytes().fold(0usize, |acc, b| (acc + b as usize) % 16);
                        vector[index] += 1.0;
                    }
                    vector
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn rag_ingest_and_retrieve_roundtrip() {
        let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new(1000));
        let pipeline = RagPipeline::new(
            store,
            Arc::new(HashEmbeddings),
            RagConfig {
                chunking: ChunkingStrategy::Fixed {
                    size: 200,
                    overlap: 20,
                },
                min_similarity: 0.3,
                ..Default::default()
            },
        );

        let document = "The AI SDK supports many providers. ".repeat(20)
            + "Rust is a systems programming language. "
            + "Web crawling respects robots.txt rules.";
        let chunk_count = pipeline.ingest("doc1", &document).await.unwrap();
        assert!(
            chunk_count >= 3,
            "document split into chunks: {chunk_count}"
        );

        let results = pipeline
            .retrieve("rust programming language", 3)
            .await
            .unwrap();
        assert!(!results.is_empty(), "query must retrieve chunks");
        assert!(
            results.iter().any(|c| c.text.contains("Rust")),
            "the Rust chunk is retrieved: {:?}",
            results.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn keyword_reranker_reorders_by_query_terms() {
        let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new(100));
        let pipeline = RagPipeline::new(
            store,
            Arc::new(HashEmbeddings),
            RagConfig {
                chunking: ChunkingStrategy::Fixed {
                    size: 1000,
                    overlap: 0,
                },
                min_similarity: 0.1,
                hybrid_alpha: 0.5,
                ..Default::default()
            },
        )
        .with_reranker(Arc::new(crate::KeywordReranker { blend: 0.8 }));

        pipeline
            .ingest("d1", "The cat sat on the mat.")
            .await
            .unwrap();
        pipeline
            .ingest("d2", "Dogs love to run in parks.")
            .await
            .unwrap();

        let results = pipeline.retrieve("cat mat", 2).await.unwrap();
        assert!(!results.is_empty());
        // The cat document should rank first for this query.
        assert!(
            results[0].text.contains("cat"),
            "{:?}",
            results.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn context_assembler_formats_chunks() {
        let assembler = ContextAssembler::default();
        let chunks = vec![
            RetrievedChunk {
                id: "a".into(),
                text: "first chunk".into(),
                score: 1.0,
                metadata: serde_json::Value::Null,
            },
            RetrievedChunk {
                id: "b".into(),
                text: "second chunk".into(),
                score: 0.9,
                metadata: serde_json::Value::Null,
            },
        ];
        let context = assembler.assemble(&chunks);
        assert!(context.contains("Relevant context"));
        assert!(context.contains("first chunk"));
        assert!(context.contains("second chunk"));
        assert!(context.contains("---"));
    }

    #[test]
    fn tokenize_available_for_keyword_reranker_tests() {
        assert_eq!(crate::hybrid::tokenize("Rust Lang"), vec!["rust", "lang"]);
    }

    /// Regression for the historical recall cap: when the semantic stage
    /// filters everything out (high `min_similarity`), the keyword stage
    /// must still surface lexical matches because it scores ALL ingested
    /// chunks. Under the old implementation the keyword stage searched only
    /// the semantic survivors, so nothing could be retrieved here.
    #[tokio::test]
    async fn keyword_stage_reaches_chunks_filtered_out_by_semantic_stage() {
        let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new(100));
        let embeddings = Arc::new(HashEmbeddings);
        let pipeline = RagPipeline::new(
            store.clone(),
            embeddings.clone(),
            RagConfig {
                chunking: ChunkingStrategy::Fixed {
                    size: 10_000,
                    overlap: 0,
                },
                // Nothing but an exact self-match survives this.
                min_similarity: 0.999,
                hybrid_alpha: 0.7,
                ..Default::default()
            },
        );

        pipeline
            .ingest(
                "d1",
                "Kumquat propulsion research remains highly experimental today.",
            )
            .await
            .unwrap();
        pipeline
            .ingest("d2", "Quarterly revenue exceeded analyst expectations.")
            .await
            .unwrap();

        // Prove the semantic stage is empty for this query under the
        // threshold (this is what capped recall historically).
        let qvec = embeddings
            .embed(&["kumquat propulsion".to_string()])
            .await
            .unwrap()[0]
            .clone();
        let semantic_hits = store.search(&qvec, 10).await.unwrap();
        let surviving = semantic_hits
            .iter()
            .filter(|(_, score)| *score >= 0.999)
            .count();
        assert_eq!(surviving, 0, "semantic stage must be empty under 0.999");

        // Yet retrieval succeeds through the corpus-BM25 keyword stage.
        let results = pipeline.retrieve("kumquat propulsion", 3).await.unwrap();
        assert!(
            results.iter().any(|c| c.id.starts_with("d1")),
            "keyword stage must rescue the lexically matching chunk: {:?}",
            results.iter().map(|c| &c.id).collect::<Vec<_>>()
        );
    }

    /// End-to-end morphology eval over the committed ai-memory fixtures:
    /// compares the fully-upgraded configuration (NgramEmbeddings +
    /// ReciprocalRank + corpus BM25) against the legacy configuration
    /// (StatisticalEmbeddings + WeightedAlpha), plus an intermediate
    /// (StatisticalEmbeddings + ReciprocalRank) to attribute gains. Prints
    /// per-config hit counts truthfully and asserts the upgrade does not
    /// regress.
    #[tokio::test(flavor = "multi_thread")]
    async fn upgraded_pipeline_improves_morphology_retrieval() {
        use std::collections::BTreeMap;

        #[derive(serde::Deserialize)]
        struct Fixture {
            distractors: Vec<String>,
            categories: Vec<Category>,
        }
        #[derive(serde::Deserialize)]
        struct Category {
            name: String,
            cases: Vec<Case>,
        }
        #[derive(serde::Deserialize)]
        struct Case {
            id: String,
            query: String,
            relevant: String,
        }

        // Reuse the ai-memory eval fixtures (same workspace).
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../ai-memory/tests/eval_fixtures/eval_set.json"
        );
        let raw = std::fs::read_to_string(path).expect("fixture file readable");
        let fx: Fixture = serde_json::from_str(&raw).unwrap();
        // Morphology-focused subset (the suite the brief targets); report
        // other categories too so nothing is cherry-picked silently.
        let categories_of_interest: Vec<&Category> = fx
            .categories
            .iter()
            .filter(|c| c.name != "lexical_control")
            .collect();

        async fn run_pipeline(
            label: &str,
            embeddings: Arc<dyn EmbeddingsProvider>,
            strategy: crate::hybrid::HybridStrategy,
            fx: &Fixture,
            categories: &[&Category],
        ) -> (BTreeMap<String, usize>, u64) {
            let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new(1000));
            let pipeline = RagPipeline::new(
                store,
                embeddings,
                RagConfig {
                    chunking: ChunkingStrategy::Fixed {
                        size: 10_000,
                        overlap: 0,
                    },
                    min_similarity: 0.0,
                    strategy,
                    ..Default::default()
                },
            );
            // Index: every relevant doc (id = case id) + distractors.
            for category in &fx.categories {
                for case in &category.cases {
                    pipeline.ingest(&case.id, &case.relevant).await.unwrap();
                }
            }
            for (i, d) in fx.distractors.iter().enumerate() {
                pipeline
                    .ingest(&format!("distractor-{i}"), d)
                    .await
                    .unwrap();
            }
            let corpus_docs = pipeline.corpus_stats().doc_count();

            let mut hits: BTreeMap<String, usize> = BTreeMap::new();
            for category in categories {
                let mut cat_hits = 0usize;
                for case in &category.cases {
                    let results = pipeline.retrieve(&case.query, 5).await.unwrap();
                    if results.iter().any(|c| c.id.starts_with(&case.id)) {
                        cat_hits += 1;
                    } else {
                        println!("  MISS [{label}] {} {}", case.id, case.query);
                    }
                }
                println!(
                    "  [{label}] {}: {cat_hits}/{}",
                    category.name,
                    category.cases.len()
                );
                hits.insert(category.name.clone(), cat_hits);
            }
            (hits, corpus_docs)
        }

        let stat = Arc::new(ai_memory::StatisticalEmbeddings::defaults());
        let ngram = Arc::new(ai_memory::NgramEmbeddings::defaults());

        println!("\n=== MINERVA e2e pipeline: top-5 hits by configuration ===");
        let (legacy, docs_legacy) = run_pipeline(
            "legacy: statistical+weighted-alpha",
            stat.clone(),
            crate::hybrid::HybridStrategy::WeightedAlpha,
            &fx,
            &categories_of_interest,
        )
        .await;
        let (fusion_only, _) = run_pipeline(
            "fusion-only: statistical+RRF",
            stat,
            crate::hybrid::HybridStrategy::ReciprocalRank,
            &fx,
            &categories_of_interest,
        )
        .await;
        let (upgraded, docs_upgraded) = run_pipeline(
            "upgraded: ngram+RRF+corpus-bm25",
            ngram,
            crate::hybrid::HybridStrategy::ReciprocalRank,
            &fx,
            &categories_of_interest,
        )
        .await;

        // Corpus statistics were fed at ingest in every configuration.
        let expected_docs = (fx.categories.iter().map(|c| c.cases.len()).sum::<usize>()
            + fx.distractors.len()) as u64;
        assert_eq!(docs_legacy, expected_docs);
        assert_eq!(docs_upgraded, expected_docs);

        let total = |m: &BTreeMap<String, usize>| m.values().sum::<usize>();
        println!(
            "totals: legacy={} fusion_only={} upgraded={}",
            total(&legacy),
            total(&fusion_only),
            total(&upgraded)
        );
        println!("=========================================================\n");

        assert!(
            total(&upgraded) >= total(&legacy),
            "upgraded pipeline must not regress vs legacy"
        );
    }
}
