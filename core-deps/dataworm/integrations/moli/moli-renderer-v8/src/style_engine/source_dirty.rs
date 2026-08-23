use std::cell::RefCell;

use indexmap::IndexSet;
use moli_selector::StyloSourceInvalidationFallbackReason;

use crate::document_runtime::DomHandle;

use super::source_id::{StyleScopeId, StyleSourceId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StyleSourceDirtyScopeSnapshot {
    records: Vec<StyleSourceDirtyScopeRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StyleSourceDirtyScopeRecord {
    scope_id: Option<StyleScopeId>,
    reason: StyleSourceDirtyReason,
    source_ids: IndexSet<StyleSourceId>,
    scoped_roots: IndexSet<DomHandle>,
    cache_write_generation_at_cleanup: Option<u64>,
    clear_all_fallback_reasons: IndexSet<StyloSourceInvalidationFallbackReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum StyleSourceDirtyReason {
    DocumentAdoptedStyleSheets,
    OwnerStyleSheet,
    LinkedStyleSheet,
    ShadowRootAdoptedStyleSheets,
    InvalidationClearAllFallback,
}

#[derive(Default)]
pub(super) struct StyleSourceDirtyScopes {
    records: RefCell<Vec<StyleSourceDirtyScopeRecord>>,
}

impl StyleSourceDirtyScopes {
    pub(super) fn record_scope(
        &self,
        scope_id: StyleScopeId,
        reason: StyleSourceDirtyReason,
        source_ids: impl IntoIterator<Item = StyleSourceId>,
        roots: impl IntoIterator<Item = DomHandle>,
        cache_write_generation_at_cleanup: u64,
    ) {
        let scoped_roots = roots.into_iter().collect::<IndexSet<_>>();
        self.records.borrow_mut().push(StyleSourceDirtyScopeRecord {
            scope_id: Some(scope_id),
            reason,
            source_ids: source_ids.into_iter().collect(),
            scoped_roots,
            cache_write_generation_at_cleanup: Some(cache_write_generation_at_cleanup),
            clear_all_fallback_reasons: IndexSet::new(),
        });
    }

    pub(super) fn record_invalidation_clear_all_fallback_reasons(
        &self,
        reasons: impl IntoIterator<Item = StyloSourceInvalidationFallbackReason>,
    ) {
        let mut records = self.records.borrow_mut();
        records
            .retain(|record| record.reason != StyleSourceDirtyReason::InvalidationClearAllFallback);
        records.push(StyleSourceDirtyScopeRecord {
            scope_id: None,
            reason: StyleSourceDirtyReason::InvalidationClearAllFallback,
            source_ids: IndexSet::new(),
            scoped_roots: IndexSet::new(),
            cache_write_generation_at_cleanup: None,
            clear_all_fallback_reasons: reasons.into_iter().collect(),
        });
    }

    pub(super) fn clear_invalidation_clear_all_fallback_reasons(&self) {
        self.records
            .borrow_mut()
            .retain(|record| record.reason != StyleSourceDirtyReason::InvalidationClearAllFallback);
    }

    pub(super) fn snapshot(&self) -> StyleSourceDirtyScopeSnapshot {
        StyleSourceDirtyScopeSnapshot {
            records: self.records.borrow().clone(),
        }
    }

    pub(super) fn clear(&self) {
        self.records.borrow_mut().clear();
    }
}

impl StyleSourceDirtyScopeSnapshot {
    pub(super) fn has_only_scoped_source_records(&self) -> bool {
        !self.records.is_empty() && self.records.iter().all(|record| record.scope_id.is_some())
    }

    pub(super) fn contains_document_scope(&self, document: DomHandle) -> bool {
        self.scope_ids().contains(&StyleScopeId::Document(document))
    }

    pub(super) fn scoped_roots_vec(&self) -> Vec<DomHandle> {
        self.scoped_roots().into_iter().collect()
    }

    pub(super) fn source_ids_vec(&self) -> Vec<StyleSourceId> {
        self.source_ids().into_iter().collect()
    }

    pub(super) fn scope_ids_vec(&self) -> Vec<StyleScopeId> {
        self.scope_ids().into_iter().collect()
    }

    pub(super) fn reasons_vec(&self) -> Vec<StyleSourceDirtyReason> {
        self.reasons().into_iter().collect()
    }

    pub(super) fn records_vec(&self) -> Vec<StyleSourceDirtyScopeRecord> {
        self.records.clone()
    }

    #[cfg(test)]
    pub(super) fn invalidation_clear_all_fallback_reasons_vec(
        &self,
    ) -> Vec<StyloSourceInvalidationFallbackReason> {
        self.invalidation_clear_all_fallback_reasons()
            .into_iter()
            .collect()
    }

    pub(super) fn scoped_roots(&self) -> IndexSet<DomHandle> {
        self.records
            .iter()
            .flat_map(|record| record.scoped_roots.iter().copied())
            .collect()
    }

    pub(super) fn cache_cleanup_covers(
        &self,
        roots: impl IntoIterator<Item = DomHandle>,
        current_cache_write_generation: u64,
    ) -> bool {
        roots.into_iter().all(|root| {
            self.records.iter().any(|record| {
                record.cache_write_generation_at_cleanup == Some(current_cache_write_generation)
                    && record.scoped_roots.contains(&root)
            })
        })
    }

    fn source_ids(&self) -> IndexSet<StyleSourceId> {
        self.records
            .iter()
            .flat_map(|record| record.source_ids.iter().cloned())
            .collect()
    }

    fn scope_ids(&self) -> IndexSet<StyleScopeId> {
        self.records
            .iter()
            .filter_map(|record| record.scope_id)
            .collect()
    }

    fn reasons(&self) -> IndexSet<StyleSourceDirtyReason> {
        self.records.iter().map(|record| record.reason).collect()
    }

    #[cfg(test)]
    fn invalidation_clear_all_fallback_reasons(
        &self,
    ) -> IndexSet<StyloSourceInvalidationFallbackReason> {
        self.records
            .iter()
            .flat_map(|record| record.clear_all_fallback_reasons.iter().copied())
            .collect()
    }
}
