use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use moli_selector::StyloSourceInvalidationFallbackReason;
use selectors::context::SelectorCaches;
use style::{
    servo_arc::Arc as ServoArc,
    stylist::{CascadeData, Stylist},
};

use crate::document_runtime::DomHandle;

use super::{
    source_dirty::{StyleSourceDirtyReason, StyleSourceDirtyScopeSnapshot, StyleSourceDirtyScopes},
    source_id::{StyleScopeId, StyleSourceId},
    system::StyleSystemCacheKey,
};

pub(super) struct RetainedStyleSystem {
    pub(super) key: StyleSystemCacheKey,
    pub(super) stylist: Stylist,
    pub(super) user_agent_cascade_data: ServoArc<CascadeData>,
    pub(super) shadow_cascade_data: Vec<(DomHandle, ServoArc<CascadeData>)>,
    pub(super) source_cascade_data: HashMap<StyleSourceId, ServoArc<CascadeData>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StyleDocumentGenerationSnapshot {
    pub(super) source_set_generation: u64,
    pub(super) computed_cache_generation: u64,
    pub(super) retained_style_system_generation: u64,
    pub(super) target_context_epoch: u64,
}

pub(super) struct StyleDocumentState {
    pub(super) retained_style_system: RefCell<Option<RetainedStyleSystem>>,
    selector_caches: RefCell<SelectorCaches>,
    source_dirty_scopes: StyleSourceDirtyScopes,
    source_set_generation: Cell<u64>,
    computed_cache_generation: Cell<u64>,
    retained_style_system_generation: Cell<u64>,
    target_context_epoch: Cell<u64>,
    #[cfg(test)]
    retained_style_system_rebuilds: Cell<u64>,
}

impl StyleDocumentState {
    pub(super) fn new() -> Self {
        Self {
            retained_style_system: RefCell::new(None),
            selector_caches: RefCell::new(SelectorCaches::default()),
            source_dirty_scopes: StyleSourceDirtyScopes::default(),
            source_set_generation: Cell::new(0),
            computed_cache_generation: Cell::new(0),
            retained_style_system_generation: Cell::new(0),
            target_context_epoch: Cell::new(0),
            #[cfg(test)]
            retained_style_system_rebuilds: Cell::new(0),
        }
    }

    pub(super) fn source_set_generation(&self) -> u64 {
        self.source_set_generation.get()
    }

    pub(super) fn bump_source_set_generation(&self) {
        self.source_set_generation
            .set(self.source_set_generation.get().saturating_add(1));
    }

    pub(super) fn computed_cache_generation(&self) -> u64 {
        self.computed_cache_generation.get()
    }

    pub(super) fn bump_computed_cache_generation(&self) {
        self.computed_cache_generation
            .set(self.computed_cache_generation().saturating_add(1));
    }

    #[cfg(test)]
    pub(super) fn retained_style_system_generation(&self) -> u64 {
        self.retained_style_system_generation.get()
    }

    pub(super) fn target_context_epoch(&self) -> u64 {
        self.target_context_epoch.get()
    }

    pub(super) fn generation_snapshot(&self) -> StyleDocumentGenerationSnapshot {
        StyleDocumentGenerationSnapshot {
            source_set_generation: self.source_set_generation(),
            computed_cache_generation: self.computed_cache_generation(),
            retained_style_system_generation: self.retained_style_system_generation.get(),
            target_context_epoch: self.target_context_epoch(),
        }
    }

    pub(super) fn bump_target_context_epoch(&self) {
        self.target_context_epoch
            .set(self.target_context_epoch().saturating_add(1));
    }

    pub(super) fn clear_retained_style_system(&self) {
        self.retained_style_system.borrow_mut().take();
        self.clear_source_dirty_scopes();
        self.clear_invalidation_clear_all_fallback_reasons();
        self.clear_selector_caches();
    }

    pub(super) fn clear_selector_caches(&self) {
        *self.selector_caches.borrow_mut() = SelectorCaches::default();
    }

    pub(super) fn take_selector_caches(&self) -> SelectorCaches {
        std::mem::take(&mut *self.selector_caches.borrow_mut())
    }

    pub(super) fn replace_selector_caches(&self, selector_caches: SelectorCaches) {
        *self.selector_caches.borrow_mut() = selector_caches;
    }

    pub(super) fn record_source_dirty_scope(
        &self,
        scope_id: StyleScopeId,
        reason: StyleSourceDirtyReason,
        source_ids: impl IntoIterator<Item = StyleSourceId>,
        roots: impl IntoIterator<Item = DomHandle>,
        cache_write_generation_at_cleanup: u64,
    ) {
        self.source_dirty_scopes.record_scope(
            scope_id,
            reason,
            source_ids,
            roots,
            cache_write_generation_at_cleanup,
        );
    }

    pub(super) fn source_dirty_scope_snapshot(&self) -> StyleSourceDirtyScopeSnapshot {
        self.source_dirty_scopes.snapshot()
    }

    pub(super) fn clear_source_dirty_scopes(&self) {
        self.source_dirty_scopes.clear();
    }

    pub(super) fn record_invalidation_clear_all_fallback_reasons(
        &self,
        reasons: impl IntoIterator<Item = StyloSourceInvalidationFallbackReason>,
    ) {
        self.source_dirty_scopes
            .record_invalidation_clear_all_fallback_reasons(reasons);
    }

    pub(super) fn clear_invalidation_clear_all_fallback_reasons(&self) {
        self.source_dirty_scopes
            .clear_invalidation_clear_all_fallback_reasons();
    }

    #[cfg(test)]
    pub(super) fn invalidation_clear_all_fallback_reasons_for_test(
        &self,
    ) -> Vec<StyloSourceInvalidationFallbackReason> {
        self.source_dirty_scope_snapshot()
            .invalidation_clear_all_fallback_reasons_vec()
    }

    pub(super) fn retained_style_system_matches(&self, key: &StyleSystemCacheKey) -> bool {
        self.retained_style_system
            .borrow()
            .as_ref()
            .is_some_and(|retained| retained.key == *key)
    }

    pub(super) fn set_retained_style_system(&self, retained: RetainedStyleSystem) {
        self.retained_style_system_generation.set(
            self.retained_style_system_generation
                .get()
                .saturating_add(1),
        );
        #[cfg(test)]
        self.retained_style_system_rebuilds
            .set(self.retained_style_system_rebuild_count().saturating_add(1));
        *self.retained_style_system.borrow_mut() = Some(retained);
    }

    #[cfg(test)]
    pub(super) fn retained_style_system_rebuild_count(&self) -> u64 {
        self.retained_style_system_rebuilds.get()
    }

    pub(super) fn with_shadow_cascade_data<R>(
        &self,
        callback: impl FnOnce(&[(DomHandle, ServoArc<CascadeData>)]) -> R,
    ) -> R {
        self.with_retained_style_system(|retained| callback(&retained.shadow_cascade_data))
    }

    pub(super) fn try_with_retained_style_system<R>(
        &self,
        callback: impl FnOnce(&RetainedStyleSystem) -> R,
    ) -> Option<R> {
        let retained = self.retained_style_system.borrow();
        retained.as_ref().map(callback)
    }

    pub(super) fn with_retained_style_system<R>(
        &self,
        callback: impl FnOnce(&RetainedStyleSystem) -> R,
    ) -> R {
        let retained = self.retained_style_system.borrow();
        let retained = retained
            .as_ref()
            .expect("retained style system should be prepared before resolving styles");
        callback(retained)
    }
}
