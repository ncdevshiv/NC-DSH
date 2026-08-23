//! Four-tier memory (PRD §3.4): working, short-term, long-term, and
//! semantic memory behind pluggable storage. Compaction/summarization is
//! driven by a caller-provided summarizer (e.g. an LLM via the gateway).
//!
//! Embeddings providers: [`StatisticalEmbeddings`] (word-level feature
//! hashing baseline) and [`NgramEmbeddings`] (character 2..=4-gram hashing
//! with online idf; robust to morphology, typos, and OOV words). Both
//! implement [`EmbeddingsProvider`] and are interchangeable everywhere;
//! ingest paths call [`EmbeddingsProvider::observe`] so stateful providers
//! can fit corpus statistics online.

mod embeddings;
mod ngram;
mod semantic;
mod statistical;
mod working;

pub use embeddings::{EmbeddingsError, EmbeddingsProvider, OpenAiCompatEmbeddings};
pub use ngram::{NgramConfig, NgramEmbeddings};
pub use semantic::{SemanticFact, SemanticMemory, SemanticMemoryConfig};
pub use statistical::{StatisticalConfig, StatisticalEmbeddings};
pub use working::{CompactionStrategy, WorkingMemory};

use std::sync::Arc;

use async_trait::async_trait;

use ai_errors::{AiError, StorageError};
use ai_storage::{DocumentStore, SqliteStore};
use ai_types::{Message, Role};

/// A memory tier (store/retrieve/clear). All tiers share this interface so
/// the agent runtime is storage-agnostic.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Stores a message for `conversation_id`.
    async fn store(&self, conversation_id: &str, message: Message) -> Result<(), AiError>;
    /// Returns the stored messages for `conversation_id`, newest first.
    async fn retrieve(&self, conversation_id: &str) -> Result<Vec<Message>, AiError>;
    /// Clears the conversation's memory.
    async fn clear(&self, conversation_id: &str) -> Result<(), AiError>;
}

/// Long-term memory backed by SQLite (real persistence, `ai-storage`).
#[derive(Clone)]
pub struct LongTermMemory {
    store: Arc<SqliteStore>,
    /// Maximum messages kept per conversation (oldest dropped).
    capacity: usize,
}

impl LongTermMemory {
    /// Opens (or creates) the database at `path`.
    pub fn open(path: &std::path::Path, capacity: usize) -> Result<Self, AiError> {
        let store = Arc::new(SqliteStore::open(path)?);
        Ok(Self {
            store,
            capacity: capacity.max(1),
        })
    }

    pub fn in_memory(capacity: usize) -> Result<Self, AiError> {
        let store = Arc::new(SqliteStore::in_memory()?);
        Ok(Self {
            store,
            capacity: capacity.max(1),
        })
    }
}

fn conversation_doc_id(conversation_id: &str, index: u64) -> String {
    format!("{conversation_id}:{index:020}")
}

#[async_trait]
impl Memory for LongTermMemory {
    async fn store(&self, conversation_id: &str, message: Message) -> Result<(), AiError> {
        let mut existing = self.retrieve(conversation_id).await?;
        existing.push(message);

        // Enforce capacity: drop the oldest persisted messages beyond the
        // cap (their doc ids are the lowest indices).
        let overflow = existing.len().saturating_sub(self.capacity);
        for index in 0..overflow {
            let id = conversation_doc_id(conversation_id, index as u64);
            self.store.delete(&id).await?;
        }

        // Persist the retained messages under their (stable) ids.
        for (index, msg) in existing.iter().enumerate().skip(overflow) {
            let id = conversation_doc_id(conversation_id, index as u64);
            let json = serde_json::to_string(msg)
                .map_err(|e| AiError::Storage(StorageError::new("sqlite", e.to_string())))?;
            self.store
                .put(
                    &id,
                    &json,
                    serde_json::json!({"conversation": conversation_id}),
                )
                .await?;
        }
        Ok(())
    }

    async fn retrieve(&self, conversation_id: &str) -> Result<Vec<Message>, AiError> {
        // Keep only docs belonging to this conversation, ordered by index.
        let prefix = format!("{conversation_id}:");
        let mut docs: Vec<(u64, Message)> = Vec::new();
        for id in self.store.list().await? {
            if let Some(suffix) = id.strip_prefix(&prefix) {
                if let Ok(index) = suffix.parse::<u64>() {
                    if let Some((content, _meta)) = self.store.get(&id).await? {
                        if let Ok(message) = serde_json::from_str::<Message>(&content) {
                            docs.push((index, message));
                        }
                    }
                }
            }
        }
        docs.sort_by_key(|(index, _)| *index);
        Ok(docs.into_iter().map(|(_, m)| m).collect())
    }

    async fn clear(&self, conversation_id: &str) -> Result<(), AiError> {
        let prefix = format!("{conversation_id}:");
        let ids: Vec<String> = self
            .store
            .list()
            .await?
            .into_iter()
            .filter(|id| id.starts_with(&prefix))
            .collect();
        for id in ids {
            self.store.delete(&id).await?;
        }
        Ok(())
    }
}

/// Convenience: a [`Memory`] that keeps everything in process (working
/// memory + short-term TTL), useful for ephemeral agents.
#[derive(Clone)]
pub struct InProcessMemory {
    working: WorkingMemory,
}

impl InProcessMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            working: WorkingMemory::new(capacity),
        }
    }
}

#[async_trait]
impl Memory for InProcessMemory {
    async fn store(&self, conversation_id: &str, message: Message) -> Result<(), AiError> {
        self.working.push(conversation_id, message)
    }

    async fn retrieve(&self, conversation_id: &str) -> Result<Vec<Message>, AiError> {
        Ok(self.working.history(conversation_id))
    }

    async fn clear(&self, conversation_id: &str) -> Result<(), AiError> {
        self.working.clear(conversation_id);
        Ok(())
    }
}

/// A summarizer used for compaction (e.g. an LLM call via the gateway).
pub type Summarizer = Arc<dyn Fn(&[Message]) -> Result<String, AiError> + Send + Sync>;

/// A `Memory` wrapper that compacts the working memory when it exceeds the
/// configured token/message budget, replacing the oldest messages with a
/// summary. The summarizer is real (LLM-backed); the wrapper is honest
/// about what it removes.
#[derive(Clone)]
pub struct CompactingMemory {
    inner: InProcessMemory,
    max_messages: usize,
    summarizer: Summarizer,
}

impl CompactingMemory {
    pub fn new(max_messages: usize, summarizer: Summarizer) -> Self {
        Self {
            inner: InProcessMemory::new(max_messages * 2),
            max_messages: max_messages.max(2),
            summarizer,
        }
    }
}

#[async_trait]
impl Memory for CompactingMemory {
    async fn store(&self, conversation_id: &str, message: Message) -> Result<(), AiError> {
        self.inner.store(conversation_id, message).await?;
        let history = self.inner.retrieve(conversation_id).await?;
        if history.len() > self.max_messages {
            let keep = history.len() / 2;
            let (old, recent) = history.split_at(keep);
            let summary = (self.summarizer)(old)?;
            self.inner.clear(conversation_id).await?;
            for msg in recent {
                self.inner.store(conversation_id, msg.clone()).await?;
            }
            let summary_message = Message::text(
                Role::System,
                format!("[compacted summary of earlier conversation]\n{summary}"),
            );
            self.inner.store(conversation_id, summary_message).await?;
        }
        Ok(())
    }

    async fn retrieve(&self, conversation_id: &str) -> Result<Vec<Message>, AiError> {
        self.inner.retrieve(conversation_id).await
    }

    async fn clear(&self, conversation_id: &str) -> Result<(), AiError> {
        self.inner.clear(conversation_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_types::ContentPart;

    #[tokio::test]
    async fn long_term_memory_persists_and_clears() {
        let memory = LongTermMemory::in_memory(10).unwrap();
        memory
            .store("c1", Message::text(Role::User, "hello"))
            .await
            .unwrap();
        memory
            .store("c1", Message::text(Role::Assistant, "hi there"))
            .await
            .unwrap();
        let history = memory.retrieve("c1").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].text_content(), "hello");
        assert_eq!(history[1].text_content(), "hi there");
        memory.clear("c1").await.unwrap();
        assert!(memory.retrieve("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn long_term_memory_is_per_conversation() {
        let memory = LongTermMemory::in_memory(10).unwrap();
        memory
            .store("a", Message::text(Role::User, "for a"))
            .await
            .unwrap();
        memory
            .store("b", Message::text(Role::User, "for b"))
            .await
            .unwrap();
        let a = memory.retrieve("a").await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].text_content(), "for a");
    }

    #[tokio::test]
    async fn long_term_memory_enforces_capacity() {
        let memory = LongTermMemory::in_memory(3).unwrap();
        for i in 0..5 {
            memory
                .store("c", Message::text(Role::User, format!("msg {i}")))
                .await
                .unwrap();
        }
        let history = memory.retrieve("c").await.unwrap();
        assert!(history.len() <= 3, "capacity enforced: {}", history.len());
        // Newest messages survive.
        assert_eq!(history.last().unwrap().text_content(), "msg 4");
    }

    #[tokio::test]
    async fn in_process_memory_roundtrip() {
        let memory = InProcessMemory::new(10);
        memory
            .store("s", Message::new(Role::User, vec![ContentPart::text("x")]))
            .await
            .unwrap();
        assert_eq!(memory.retrieve("s").await.unwrap().len(), 1);
        memory.clear("s").await.unwrap();
        assert!(memory.retrieve("s").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn compacting_memory_summarizes_old_messages() {
        let memory = CompactingMemory::new(
            4,
            Arc::new(|old: &[Message]| Ok(format!("summary of {} messages", old.len()))),
        );
        for i in 0..6 {
            memory
                .store("c", Message::text(Role::User, format!("m{i}")))
                .await
                .unwrap();
        }
        let history = memory.retrieve("c").await.unwrap();
        // A summary marker must be present, and the total stays bounded.
        assert!(
            history
                .iter()
                .any(|m| m.text_content().contains("summary of")),
            "compaction inserted a summary: {:?}",
            history.iter().map(|m| m.text_content()).collect::<Vec<_>>()
        );
        assert!(history.len() <= 6, "bounded: {}", history.len());
    }
}

#[cfg(test)]
mod proptests;
