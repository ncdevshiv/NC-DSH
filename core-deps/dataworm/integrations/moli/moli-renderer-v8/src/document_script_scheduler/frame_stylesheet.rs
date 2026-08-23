use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    dom::NodeId,
    frame_owner_model::{DocumentLoadDelayTokenId, FrameDocumentOwner},
    stylesheet_blocking::{
        DocumentBlockingStylesheetSignature, DocumentOwnedBlockingStylesheetDiscoveryInput,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameDocumentBlockingStylesheetStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Debug)]
struct FrameDocumentBlockingStylesheetEntry {
    node_ids: HashSet<NodeId>,
    status: FrameDocumentBlockingStylesheetStatus,
    load_delay_token: Option<DocumentLoadDelayTokenId>,
}

#[derive(Debug, Default)]
pub(crate) struct FrameDocumentBlockingStylesheetStore {
    documents: BTreeMap<
        FrameDocumentOwner,
        HashMap<DocumentBlockingStylesheetSignature, FrameDocumentBlockingStylesheetEntry>,
    >,
}

impl FrameDocumentBlockingStylesheetStore {
    pub(crate) fn discover(
        &mut self,
        owner: FrameDocumentOwner,
        input: &DocumentOwnedBlockingStylesheetDiscoveryInput,
        acquire_load_delay: impl FnOnce() -> Option<DocumentLoadDelayTokenId>,
    ) -> bool {
        let signature = input.signature().clone();
        let entries = self.documents.entry(owner).or_default();
        if let Some(entry) = entries.get_mut(&signature) {
            entry.node_ids.insert(input.node_id());
            tracing::debug!(
                owner = ?owner,
                node_id = ?input.node_id(),
                signature = ?signature,
                status = ?entry.status,
                "child parser stylesheet discovery joined existing document-owned readiness"
            );
            return false;
        }
        let Some(load_delay_token) = acquire_load_delay() else {
            tracing::warn!(
                owner = ?owner,
                node_id = ?input.node_id(),
                signature = ?signature,
                "rejecting child parser stylesheet without a document load-delay token"
            );
            return false;
        };
        entries.insert(
            signature.clone(),
            FrameDocumentBlockingStylesheetEntry {
                node_ids: HashSet::from([input.node_id()]),
                status: FrameDocumentBlockingStylesheetStatus::Pending,
                load_delay_token: Some(load_delay_token),
            },
        );
        tracing::debug!(
            owner = ?owner,
            node_id = ?input.node_id(),
            signature = ?signature,
            ?load_delay_token,
            pending_count = entries
                .values()
                .filter(|entry| entry.status == FrameDocumentBlockingStylesheetStatus::Pending)
                .count(),
            "accepted child parser blocking stylesheet for document owner"
        );
        true
    }

    pub(crate) fn apply_completion(
        &mut self,
        owner: FrameDocumentOwner,
        signature: &DocumentBlockingStylesheetSignature,
        successful: bool,
    ) -> Option<DocumentLoadDelayTokenId> {
        let entries = self.documents.get_mut(&owner)?;
        let entry = entries.get_mut(signature)?;
        if entry.status != FrameDocumentBlockingStylesheetStatus::Pending {
            return None;
        }
        entry.status = if successful {
            FrameDocumentBlockingStylesheetStatus::Ready
        } else {
            FrameDocumentBlockingStylesheetStatus::Failed
        };
        let load_delay_token = entry.load_delay_token.take()?;
        tracing::debug!(
            owner = ?owner,
            signature = ?signature,
            successful,
            ?load_delay_token,
            node_count = entry.node_ids.len(),
            "applied child parser stylesheet terminal to document-owned readiness"
        );
        Some(load_delay_token)
    }

    pub(crate) fn blocks_signatures<'a>(
        &self,
        owner: FrameDocumentOwner,
        signatures: impl IntoIterator<Item = &'a DocumentBlockingStylesheetSignature>,
    ) -> bool {
        let Some(entries) = self.documents.get(&owner) else {
            return false;
        };
        signatures.into_iter().any(|signature| {
            entries
                .get(signature)
                .is_some_and(|entry| entry.status == FrameDocumentBlockingStylesheetStatus::Pending)
        })
    }

    pub(crate) fn has_pending(&self, owner: FrameDocumentOwner) -> bool {
        self.documents.get(&owner).is_some_and(|entries| {
            entries
                .values()
                .any(|entry| entry.status == FrameDocumentBlockingStylesheetStatus::Pending)
        })
    }

    pub(crate) fn node_ids_for_signature(
        &self,
        owner: FrameDocumentOwner,
        signature: &DocumentBlockingStylesheetSignature,
    ) -> Vec<NodeId> {
        self.documents
            .get(&owner)
            .and_then(|entries| entries.get(signature))
            .map(|entry| entry.node_ids.iter().copied().collect())
            .unwrap_or_default()
    }

    pub(crate) fn remove_document(&mut self, owner: FrameDocumentOwner) -> usize {
        let removed = self
            .documents
            .remove(&owner)
            .map_or(0, |entries| entries.len());
        if removed != 0 {
            tracing::debug!(
                owner = ?owner,
                removed,
                "retired child parser stylesheet readiness with document owner"
            );
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_owner_model::{DocumentId, LocalWindowId};
    use moli_stylesheet_blocking::{
        DocumentOwnedBlockingStylesheet, DocumentOwnedBlockingStylesheetCandidate,
        StylesheetFetchOptions,
    };
    use url::Url;

    fn owner() -> FrameDocumentOwner {
        FrameDocumentOwner::new(LocalWindowId(1), DocumentId(2))
    }

    fn input(node_id: usize, url: &str) -> DocumentOwnedBlockingStylesheetDiscoveryInput {
        let blocker = DocumentOwnedBlockingStylesheet::from_candidate(
            &DocumentOwnedBlockingStylesheetCandidate::Link {
                node_id: NodeId::new(node_id),
                url: Url::parse(url).expect("stylesheet url"),
                options: StylesheetFetchOptions::default(),
            },
        );
        DocumentOwnedBlockingStylesheetDiscoveryInput::from(&blocker)
    }

    #[test]
    fn child_stylesheet_readiness_is_owner_scoped_and_failure_unblocks() {
        let owner = owner();
        let mut store = FrameDocumentBlockingStylesheetStore::default();
        let input = input(3, "https://styles.test/blocked.css");

        let load_delay_token = DocumentLoadDelayTokenId(7);
        assert!(store.discover(owner, &input, || Some(load_delay_token)));
        assert!(
            !store.discover(owner, &input, || {
                panic!("joined stylesheet discovery must not acquire another load-delay token")
            }),
            "duplicate stylesheet discovery should join the original owner entry"
        );
        assert!(store.blocks_signatures(owner, [input.signature()]));
        assert_eq!(
            store.apply_completion(owner, input.signature(), false),
            Some(load_delay_token)
        );
        assert!(!store.blocks_signatures(owner, [input.signature()]));
        assert_eq!(store.apply_completion(owner, input.signature(), true), None);
        assert_eq!(store.remove_document(owner), 1);
        assert!(!store.has_pending(owner));

        let replacement_owner =
            FrameDocumentOwner::new(owner.local_window_id, DocumentId(owner.document_id.0 + 1));
        assert!(store.discover(replacement_owner, &input, || {
            Some(DocumentLoadDelayTokenId(8))
        }));
        assert!(
            store
                .apply_completion(owner, input.signature(), true)
                .is_none(),
            "retired owner completion must not update replacement readiness"
        );
        assert!(store.blocks_signatures(replacement_owner, [input.signature()]));
    }
}
