//! Semantic memory: facts stored with embeddings and retrieved by
//! similarity (PRD §3.4 semantic tier).

use std::sync::Arc;

use parking_lot::RwLock;

use ai_errors::{AiError, StorageError};
use ai_storage::{VectorEntry, VectorStore};

use crate::embeddings::EmbeddingsProvider;

/// A semantic fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFact {
    pub id: String,
    pub text: String,
    /// Metadata (e.g. `{"type": "user-preference"}`).
    pub metadata: serde_json::Value,
}

/// Configuration for semantic memory.
#[derive(Debug, Clone)]
pub struct SemanticMemoryConfig {
    /// Cosine similarity threshold for retrieval.
    pub min_similarity: f32,
    /// Maximum stored facts (bounded memory).
    pub capacity: usize,
}

impl Default for SemanticMemoryConfig {
    fn default() -> Self {
        Self {
            min_similarity: 0.5,
            capacity: 10_000,
        }
    }
}

/// Semantic memory over an in-process vector store with a real embeddings
/// provider for encoding.
pub struct SemanticMemory {
    store: ai_storage::InMemoryVectorStore,
    embeddings: Arc<dyn EmbeddingsProvider>,
    config: SemanticMemoryConfig,
    facts: Arc<RwLock<std::collections::HashMap<String, SemanticFact>>>,
}

impl SemanticMemory {
    pub fn new(embeddings: Arc<dyn EmbeddingsProvider>, config: SemanticMemoryConfig) -> Self {
        Self {
            store: ai_storage::InMemoryVectorStore::new(config.capacity),
            embeddings,
            config,
            facts: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Stores a fact, embedding it via the provider.
    pub async fn store(&self, fact: SemanticFact) -> Result<(), AiError> {
        let vector = self
            .embeddings
            .embed(std::slice::from_ref(&fact.text))
            .await
            .map_err(|e| AiError::Storage(StorageError::new("semantic", e.to_string())))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AiError::Storage(StorageError::new(
                    "semantic",
                    "embeddings returned no vector",
                ))
            })?;

        self.store
            .upsert(VectorEntry {
                id: fact.id.clone(),
                vector,
                payload: serde_json::json!({"text": fact.text, "metadata": fact.metadata}),
            })
            .await?;
        self.facts.write().insert(fact.id.clone(), fact);
        Ok(())
    }

    /// Retrieves facts similar to `query`, most similar first.
    pub async fn retrieve(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<(SemanticFact, f32)>, AiError> {
        let vector = self
            .embeddings
            .embed(&[query.to_string()])
            .await
            .map_err(|e| AiError::Storage(StorageError::new("semantic", e.to_string())))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AiError::Storage(StorageError::new(
                    "semantic",
                    "embeddings returned no vector",
                ))
            })?;

        let results = self.store.search(&vector, top_k).await?;
        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= self.config.min_similarity)
            .filter_map(|(entry, score)| {
                let fact = self.facts.read().get(&entry.id).cloned()?;
                Some((fact, score))
            })
            .collect())
    }

    /// Retrieval with a metadata filter applied after similarity search.
    pub async fn retrieve_filtered(
        &self,
        query: &str,
        top_k: usize,
        filter: impl Fn(&SemanticFact) -> bool,
    ) -> Result<Vec<(SemanticFact, f32)>, AiError> {
        Ok(self
            .retrieve(query, top_k * 4)
            .await?
            .into_iter()
            .filter(|(fact, _)| filter(fact))
            .take(top_k)
            .collect())
    }

    pub async fn len(&self) -> Result<usize, AiError> {
        self.store.len().await
    }

    pub async fn is_empty(&self) -> Result<bool, AiError> {
        Ok(self.len().await? == 0)
    }

    pub async fn delete(&self, id: &str) -> Result<(), AiError> {
        self.store.delete(id).await?;
        self.facts.write().remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::EmbeddingsError;
    use std::sync::Arc;

    /// A deterministic embeddings provider for tests: maps tokens to
    /// orthogonal one-hot vectors so similarity is meaningful without any
    /// external service.
    struct TokenEmbeddings;

    fn token_vector(text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; 8];
        for word in text.split_whitespace() {
            let index = word.bytes().fold(0usize, |acc, b| (acc + b as usize) % 8);
            vector[index] += 1.0;
        }
        vector
    }

    #[async_trait::async_trait]
    impl EmbeddingsProvider for TokenEmbeddings {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingsError> {
            Ok(texts.iter().map(|t| token_vector(t)).collect())
        }
    }

    #[tokio::test]
    async fn stores_and_retrieves_similar_facts() {
        let memory =
            SemanticMemory::new(Arc::new(TokenEmbeddings), SemanticMemoryConfig::default());
        memory
            .store(SemanticFact {
                id: "f1".into(),
                text: "the sky is blue".into(),
                metadata: serde_json::json!({"type": "observation"}),
            })
            .await
            .unwrap();
        memory
            .store(SemanticFact {
                id: "f2".into(),
                text: "the ocean is salty".into(),
                metadata: serde_json::json!({"type": "observation"}),
            })
            .await
            .unwrap();

        let results = memory.retrieve("blue sky", 2).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0.id, "f1", "most similar fact first");

        // Unrelated query returns nothing above the threshold.
        let none = memory.retrieve("ghi abc", 2).await.unwrap();
        assert!(none.is_empty(), "{none:?}");
        assert_eq!(memory.len().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn metadata_filter_applies() {
        let memory =
            SemanticMemory::new(Arc::new(TokenEmbeddings), SemanticMemoryConfig::default());
        memory
            .store(SemanticFact {
                id: "p".into(),
                text: "prefers dark mode".into(),
                metadata: serde_json::json!({"type": "user-preference"}),
            })
            .await
            .unwrap();
        memory
            .store(SemanticFact {
                id: "o".into(),
                text: "dark theme looks nice".into(),
                metadata: serde_json::json!({"type": "observation"}),
            })
            .await
            .unwrap();

        let filtered = memory
            .retrieve_filtered("dark preference", 4, |f| {
                f.metadata.get("type") == Some(&serde_json::json!("user-preference"))
            })
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0.id, "p");
    }
}
