//! Working memory: a bounded per-conversation message history.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use ai_errors::{AiError, ValidationError};
use ai_types::Message;

/// How to treat old messages when the working memory is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionStrategy {
    /// Drop the oldest messages (sliding window).
    #[default]
    SlidingWindow,
    /// Summarize the oldest half (requires a summarizer; see
    /// [`crate::CompactingMemory`]).
    Summarize,
}

/// In-process working memory: bounded per-conversation histories.
#[derive(Clone, Default)]
pub struct WorkingMemory {
    conversations: Arc<RwLock<HashMap<String, Vec<Message>>>>,
    capacity: usize,
    strategy: CompactionStrategy,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            conversations: Arc::new(RwLock::new(HashMap::new())),
            capacity: capacity.max(1),
            strategy: CompactionStrategy::SlidingWindow,
        }
    }

    pub fn with_strategy(mut self, strategy: CompactionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Appends a message, applying the configured strategy at capacity.
    pub fn push(&self, conversation_id: &str, message: Message) -> Result<(), AiError> {
        let mut conversations = self.conversations.write();
        let history = conversations
            .entry(conversation_id.to_string())
            .or_default();
        if history.len() >= self.capacity {
            match self.strategy {
                CompactionStrategy::SlidingWindow => {
                    // Drop the oldest message to make room.
                    history.remove(0);
                }
                CompactionStrategy::Summarize => {
                    return Err(AiError::Validation(ValidationError::new(
                        "working memory full; Summarize strategy requires the \
                         CompactingMemory wrapper (summarizer)",
                    )));
                }
            }
        }
        history.push(message);
        Ok(())
    }

    /// Current history for a conversation, oldest first.
    pub fn history(&self, conversation_id: &str) -> Vec<Message> {
        self.conversations
            .read()
            .get(conversation_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn clear(&self, conversation_id: &str) {
        self.conversations.write().remove(conversation_id);
    }

    pub fn len(&self, conversation_id: &str) -> usize {
        self.history(conversation_id).len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_types::Role;

    #[test]
    fn sliding_window_drops_oldest() {
        let memory = WorkingMemory::new(3);
        for i in 0..5 {
            memory
                .push("c", Message::text(Role::User, format!("m{i}")))
                .unwrap();
        }
        let history = memory.history("c");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].text_content(), "m2");
        assert_eq!(history[2].text_content(), "m4");
    }

    #[test]
    fn conversations_are_isolated() {
        let memory = WorkingMemory::new(10);
        memory.push("a", Message::text(Role::User, "x")).unwrap();
        memory.push("b", Message::text(Role::User, "y")).unwrap();
        assert_eq!(memory.len("a"), 1);
        assert_eq!(memory.len("b"), 1);
        memory.clear("a");
        assert_eq!(memory.len("a"), 0);
        assert_eq!(memory.len("b"), 1);
    }

    #[test]
    fn summarize_strategy_requires_wrapper() {
        let memory = WorkingMemory::new(2).with_strategy(CompactionStrategy::Summarize);
        memory.push("c", Message::text(Role::User, "1")).unwrap();
        memory.push("c", Message::text(Role::User, "2")).unwrap();
        let err = memory
            .push("c", Message::text(Role::User, "3"))
            .unwrap_err();
        assert!(err.to_string().contains("CompactingMemory"), "{err}");
    }
}
