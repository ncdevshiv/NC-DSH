use std::cell::RefCell;

use indexmap::IndexSet;

use super::{
    StyleSourceId,
    cause::{PendingStyleInvalidationMergeClass, PendingStyleInvalidationWorkKind},
    target_queries::{
        PendingStyleInvalidationTargetQueries, PendingStyleInvalidationTargetSourceIdSink,
        merge_pending_target_queries,
    },
};

#[derive(Default)]
pub(super) struct PendingStyleInvalidations {
    work_items: RefCell<Vec<PendingStyleInvalidationWork>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingStyleInvalidationWork {
    pub(super) kind: PendingStyleInvalidationWorkKind,
    pub(super) target_queries: Vec<PendingStyleInvalidationTargetQueries>,
    merge_class: PendingStyleInvalidationMergeClass,
}

impl PendingStyleInvalidationWork {
    pub(super) fn new(
        kind: PendingStyleInvalidationWorkKind,
        target_queries: Vec<PendingStyleInvalidationTargetQueries>,
        merge_class: PendingStyleInvalidationMergeClass,
    ) -> Self {
        Self {
            kind,
            target_queries,
            merge_class,
        }
    }
}

#[cfg(test)]
impl PendingStyleInvalidationWork {
    pub(super) fn for_test(
        kind: PendingStyleInvalidationWorkKind,
        target_queries: Vec<PendingStyleInvalidationTargetQueries>,
    ) -> Self {
        Self {
            kind,
            target_queries,
            merge_class: PendingStyleInvalidationMergeClass::None,
        }
    }
}

pub(super) struct PendingStyleInvalidationBatch {
    pub(super) work_items: Vec<PendingStyleInvalidationWork>,
}

#[derive(Default)]
struct PendingStyleInvalidationBatchSourceIds {
    source_ids: IndexSet<StyleSourceId>,
}

impl PendingStyleInvalidations {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn extend_work(&self, work: PendingStyleInvalidationWork) {
        if work.target_queries.is_empty() {
            return;
        }
        let mut work_items = self.work_items.borrow_mut();
        if let Some(PendingStyleInvalidationWork {
            merge_class: existing_merge_class,
            target_queries: existing_target_queries,
            ..
        }) = work_items.last_mut()
            && *existing_merge_class != PendingStyleInvalidationMergeClass::None
            && *existing_merge_class == work.merge_class
        {
            merge_pending_target_queries(existing_target_queries, work.target_queries);
            return;
        }
        work_items.push(work);
    }

    pub(super) fn take(&self) -> PendingStyleInvalidationBatch {
        PendingStyleInvalidationBatch {
            work_items: self.work_items.take(),
        }
    }

    #[cfg(test)]
    pub(super) fn work_item_count_for_test(&self) -> usize {
        self.work_items.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn work_kind_names_for_test(&self) -> Vec<&'static str> {
        self.work_items
            .borrow()
            .iter()
            .map(|work_item| work_item.kind.name_for_test())
            .collect()
    }

    pub(super) fn clear(&self) {
        self.work_items.borrow_mut().clear();
    }
}

impl PendingStyleInvalidationBatch {
    pub(super) fn has_work(&self) -> bool {
        !self.work_items.is_empty()
    }

    pub(super) fn stylesheet_source_ids(&self) -> IndexSet<StyleSourceId> {
        let mut source_ids = PendingStyleInvalidationBatchSourceIds::default();
        for work_item in &self.work_items {
            for target_query in &work_item.target_queries {
                target_query.record_stylesheet_source_id_into(&mut source_ids);
            }
        }
        source_ids.source_ids
    }
}

impl PendingStyleInvalidationTargetSourceIdSink for PendingStyleInvalidationBatchSourceIds {
    fn record_pending_target_stylesheet_source_id(&mut self, source_id: &StyleSourceId) {
        self.source_ids.insert(source_id.clone());
    }
}
