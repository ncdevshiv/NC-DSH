//! Derived owner-local index for stable residences with absolute deadlines.
//!
//! The index is deliberately domain-neutral. It does not decide whether a
//! deadline represents a JavaScript timer, renderer maintenance, or another
//! future owner concern; callers preserve that identity by owning separate
//! index instances. The authoritative deadline remains in the corresponding
//! residence, and callers remove/rebuild this derived entry at their ownership
//! boundaries.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::time::Instant;

pub(super) struct OwnerDeadlineIndex<Token> {
    by_deadline: BTreeMap<Instant, HashSet<Token>>,
    deadline_by_token: HashMap<Token, Instant>,
}

impl<Token> Default for OwnerDeadlineIndex<Token> {
    fn default() -> Self {
        Self {
            by_deadline: BTreeMap::new(),
            deadline_by_token: HashMap::new(),
        }
    }
}

impl<Token> OwnerDeadlineIndex<Token>
where
    Token: Copy + Eq + Hash,
{
    pub(super) fn insert(&mut self, token: Token, deadline: Instant) {
        self.remove(token);
        self.by_deadline.entry(deadline).or_default().insert(token);
        self.deadline_by_token.insert(token, deadline);
    }

    pub(super) fn remove(&mut self, token: Token) {
        let Some(deadline) = self.deadline_by_token.remove(&token) else {
            return;
        };
        let should_remove_deadline = if let Some(tokens) = self.by_deadline.get_mut(&deadline) {
            tokens.remove(&token);
            tokens.is_empty()
        } else {
            false
        };
        if should_remove_deadline {
            self.by_deadline.remove(&deadline);
        }
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.by_deadline.keys().next().copied()
    }

    pub(super) fn snapshot_due_tokens(&self, due_at_or_before: Instant) -> Vec<Token> {
        self.by_deadline
            .range(..=due_at_or_before)
            .flat_map(|(_, tokens)| tokens.iter().copied())
            .collect()
    }

    #[cfg(debug_assertions)]
    pub(super) fn deadline_for(&self, token: Token) -> Option<Instant> {
        self.deadline_by_token.get(&token).copied()
    }

    #[cfg(debug_assertions)]
    pub(super) fn indexed_tokens(&self) -> impl Iterator<Item = Token> + '_ {
        self.deadline_by_token.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn deadline_index_replaces_and_removes_derived_entries() {
        let now = Instant::now();
        let later = now
            .checked_add(Duration::from_secs(1))
            .expect("test deadline should fit");
        let mut index = OwnerDeadlineIndex::default();

        index.insert(1_u8, later);
        index.insert(1_u8, now);

        assert_eq!(index.next_deadline(), Some(now));
        assert_eq!(index.snapshot_due_tokens(now), vec![1]);
        index.remove(1);
        assert_eq!(index.next_deadline(), None);
    }

    #[test]
    fn deadline_index_keeps_all_tokens_at_the_same_deadline() {
        let deadline = Instant::now();
        let mut index = OwnerDeadlineIndex::default();

        index.insert(1_u8, deadline);
        index.insert(2_u8, deadline);
        let mut due = index.snapshot_due_tokens(deadline);
        due.sort_unstable();

        assert_eq!(due, vec![1, 2]);
    }
}
