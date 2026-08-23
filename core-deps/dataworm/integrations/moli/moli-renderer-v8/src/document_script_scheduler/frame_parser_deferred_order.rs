use std::collections::{BTreeMap, VecDeque};

use crate::frame_owner_model::FrameDocumentOwner;

use super::{ParserPendingScriptId, ParserPendingScriptKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameParserDeferredScriptKind {
    Classic,
    Module,
}

impl FrameParserDeferredScriptKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Module => "module",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameParserDeferredScriptOrderEntry {
    key: ParserPendingScriptKey,
    kind: FrameParserDeferredScriptKind,
}

impl FrameParserDeferredScriptOrderEntry {
    pub(crate) fn classic(key: ParserPendingScriptKey) -> Self {
        Self {
            key,
            kind: FrameParserDeferredScriptKind::Classic,
        }
    }

    pub(crate) fn module(pending_script_id: ParserPendingScriptId<FrameDocumentOwner>) -> Self {
        Self {
            key: pending_script_id.key(),
            kind: FrameParserDeferredScriptKind::Module,
        }
    }

    pub(crate) fn key(self) -> ParserPendingScriptKey {
        self.key
    }

    pub(crate) fn kind(self) -> FrameParserDeferredScriptKind {
        self.kind
    }

    pub(crate) fn pending_module_script_id(
        self,
        owner: FrameDocumentOwner,
    ) -> Option<ParserPendingScriptId<FrameDocumentOwner>> {
        (self.kind == FrameParserDeferredScriptKind::Module)
            .then(|| ParserPendingScriptId::from_key(owner, self.key))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameParserDeferredScriptOrderState {
    Pending,
    InFlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameParserDeferredScriptOrderSlot {
    entry: FrameParserDeferredScriptOrderEntry,
    state: FrameParserDeferredScriptOrderState,
}

impl FrameParserDeferredScriptOrderSlot {
    fn pending(entry: FrameParserDeferredScriptOrderEntry) -> Self {
        Self {
            entry,
            state: FrameParserDeferredScriptOrderState::Pending,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrameParserDeferredScriptOrderStore {
    documents: BTreeMap<FrameDocumentOwner, VecDeque<FrameParserDeferredScriptOrderSlot>>,
}

impl FrameParserDeferredScriptOrderStore {
    pub(crate) fn register(
        &mut self,
        owner: FrameDocumentOwner,
        entry: FrameParserDeferredScriptOrderEntry,
    ) -> bool {
        let queue = self.documents.entry(owner).or_default();
        if let Some(existing) = queue
            .iter()
            .find(|existing| existing.entry.key == entry.key)
        {
            if existing.entry.kind != entry.kind {
                tracing::warn!(
                    owner = ?owner,
                    parser_position = entry.key.parser_position(),
                    script_node_id = ?entry.key.script_node_id(),
                    existing_kind = existing.entry.kind.as_str(),
                    requested_kind = entry.kind.as_str(),
                    "rejecting conflicting child parser-deferred order registration"
                );
            } else {
                tracing::debug!(
                    owner = ?owner,
                    parser_position = entry.key.parser_position(),
                    script_node_id = ?entry.key.script_node_id(),
                    kind = entry.kind.as_str(),
                    "child parser-deferred order entry was already registered"
                );
            }
            return false;
        }

        let insert_at = queue
            .iter()
            .position(|queued| queued.entry.key > entry.key)
            .unwrap_or(queue.len());
        queue.insert(
            insert_at,
            FrameParserDeferredScriptOrderSlot::pending(entry),
        );
        tracing::debug!(
            owner = ?owner,
            parser_position = entry.key.parser_position(),
            script_node_id = ?entry.key.script_node_id(),
            kind = entry.kind.as_str(),
            queue_len = queue.len(),
            "registered child parser-deferred script in cross-kind document order"
        );
        true
    }

    pub(crate) fn head(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<FrameParserDeferredScriptOrderEntry> {
        Some(self.documents.get(&owner)?.front()?.entry)
    }

    pub(crate) fn pending_head(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<FrameParserDeferredScriptOrderEntry> {
        let head = self.documents.get(&owner)?.front()?;
        (head.state == FrameParserDeferredScriptOrderState::Pending).then_some(head.entry)
    }

    pub(crate) fn in_flight_head(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<FrameParserDeferredScriptOrderEntry> {
        let head = self.documents.get(&owner)?.front()?;
        (head.state == FrameParserDeferredScriptOrderState::InFlight).then_some(head.entry)
    }

    pub(crate) fn mark_head_in_flight(
        &mut self,
        owner: FrameDocumentOwner,
        expected: FrameParserDeferredScriptOrderEntry,
    ) -> bool {
        let Some(head) = self
            .documents
            .get_mut(&owner)
            .and_then(|queue| queue.front_mut())
        else {
            return false;
        };
        if head.entry != expected || head.state != FrameParserDeferredScriptOrderState::Pending {
            tracing::warn!(
                owner = ?owner,
                parser_position = expected.key.parser_position(),
                script_node_id = ?expected.key.script_node_id(),
                kind = expected.kind.as_str(),
                actual_head = ?head,
                "refusing to claim a non-pending child parser-deferred order head"
            );
            return false;
        }
        head.state = FrameParserDeferredScriptOrderState::InFlight;
        tracing::debug!(
            owner = ?owner,
            parser_position = expected.key.parser_position(),
            script_node_id = ?expected.key.script_node_id(),
            kind = expected.kind.as_str(),
            "claimed child parser-deferred order head for DocumentScriptReady"
        );
        true
    }

    pub(crate) fn restore_in_flight_head(
        &mut self,
        owner: FrameDocumentOwner,
        expected: FrameParserDeferredScriptOrderEntry,
    ) -> bool {
        let Some(head) = self
            .documents
            .get_mut(&owner)
            .and_then(|queue| queue.front_mut())
        else {
            return false;
        };
        if head.entry != expected || head.state != FrameParserDeferredScriptOrderState::InFlight {
            return false;
        }
        head.state = FrameParserDeferredScriptOrderState::Pending;
        tracing::debug!(
            owner = ?owner,
            parser_position = expected.key.parser_position(),
            script_node_id = ?expected.key.script_node_id(),
            kind = expected.kind.as_str(),
            "restored child parser-deferred order head after failed promotion"
        );
        true
    }

    pub(crate) fn release_in_flight_head(
        &mut self,
        owner: FrameDocumentOwner,
        expected: FrameParserDeferredScriptOrderEntry,
    ) -> bool {
        let Some(queue) = self.documents.get_mut(&owner) else {
            return false;
        };
        if !queue.front().is_some_and(|head| {
            head.entry == expected && head.state == FrameParserDeferredScriptOrderState::InFlight
        }) {
            tracing::warn!(
                owner = ?owner,
                parser_position = expected.key.parser_position(),
                script_node_id = ?expected.key.script_node_id(),
                kind = expected.kind.as_str(),
                actual_head = ?queue.front(),
                "refusing to release a child parser-deferred script without its in-flight order slot"
            );
            return false;
        }
        queue.pop_front();
        let remaining = queue.len();
        if queue.is_empty() {
            self.documents.remove(&owner);
        }
        tracing::debug!(
            owner = ?owner,
            parser_position = expected.key.parser_position(),
            script_node_id = ?expected.key.script_node_id(),
            kind = expected.kind.as_str(),
            remaining,
            "released completed child parser-deferred cross-kind document-order head"
        );
        true
    }

    pub(crate) fn remove_pending(
        &mut self,
        owner: FrameDocumentOwner,
        expected: FrameParserDeferredScriptOrderEntry,
    ) -> bool {
        let Some(queue) = self.documents.get_mut(&owner) else {
            return false;
        };
        let Some(position) = queue.iter().position(|slot| {
            slot.entry == expected && slot.state == FrameParserDeferredScriptOrderState::Pending
        }) else {
            return false;
        };
        queue.remove(position);
        let remaining = queue.len();
        if queue.is_empty() {
            self.documents.remove(&owner);
        }
        tracing::debug!(
            owner = ?owner,
            parser_position = expected.key.parser_position(),
            script_node_id = ?expected.key.script_node_id(),
            kind = expected.kind.as_str(),
            remaining,
            "removed unstarted child parser-deferred order entry during acceptance rollback"
        );
        true
    }

    pub(crate) fn remove_document(&mut self, owner: FrameDocumentOwner) -> usize {
        let removed = self.documents.remove(&owner).map_or(0, |queue| queue.len());
        if removed != 0 {
            tracing::debug!(
                owner = ?owner,
                removed,
                "retired child parser-deferred order entries with document owner"
            );
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::NodeId;
    use crate::frame_owner_model::{DocumentId, LocalWindowId};

    fn owner() -> FrameDocumentOwner {
        FrameDocumentOwner::new(LocalWindowId(1), DocumentId(2))
    }

    fn key(position: usize, node: usize) -> ParserPendingScriptKey {
        ParserPendingScriptKey::from_parts_for_test(position, NodeId::new(node))
    }

    #[test]
    fn child_parser_deferred_order_is_cross_kind_and_position_owned() {
        let owner = owner();
        let mut store = FrameParserDeferredScriptOrderStore::default();
        let later_classic = FrameParserDeferredScriptOrderEntry::classic(key(20, 2));
        let earlier_module = FrameParserDeferredScriptOrderEntry::module(
            ParserPendingScriptId::from_key(owner, key(10, 1)),
        );

        assert!(store.register(owner, later_classic));
        assert!(store.register(owner, earlier_module));
        assert_eq!(store.head(owner), Some(earlier_module));
        assert_eq!(store.pending_head(owner), Some(earlier_module));
        assert!(!store.mark_head_in_flight(owner, later_classic));
        assert!(store.mark_head_in_flight(owner, earlier_module));
        assert_eq!(store.pending_head(owner), None);
        assert_eq!(store.in_flight_head(owner), Some(earlier_module));
        assert!(!store.mark_head_in_flight(owner, earlier_module));
        assert!(!store.release_in_flight_head(owner, later_classic));
        assert!(store.release_in_flight_head(owner, earlier_module));
        assert_eq!(store.head(owner), Some(later_classic));
        assert!(store.mark_head_in_flight(owner, later_classic));
        assert!(store.restore_in_flight_head(owner, later_classic));
        assert_eq!(store.pending_head(owner), Some(later_classic));
        assert!(store.mark_head_in_flight(owner, later_classic));
        assert!(store.release_in_flight_head(owner, later_classic));
        assert_eq!(store.head(owner), None);
    }
}
