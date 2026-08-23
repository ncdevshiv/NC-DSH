use moli_selector::{
    StyloPlannedFallbackRootInvalidationTarget, StyloStyleSourceScope as StyleSourceScope,
    stylo_fallback_roots_plan,
};

use crate::{document_runtime::DomHandle, dom::native::DomHost};

use super::{cause::PendingCauseFallback, target_queries::PendingStyleInvalidationTargetQueries};

pub(super) struct PendingBoundaryFallbackTarget {
    fallback_target: StyloPlannedFallbackRootInvalidationTarget,
}

impl PendingCauseFallback {
    pub(super) fn target_queries_for_structural_boundary_roots(
        &self,
        host: &DomHost,
        roots: impl IntoIterator<Item = DomHandle>,
    ) -> Vec<PendingStyleInvalidationTargetQueries> {
        let mut target_queries = target_queries_for_planned_fallback_target(
            host,
            stylo_fallback_roots_plan(roots.into_iter().collect(), std::iter::empty()),
        );
        self.merge_cause_roots_into_source_dependency_target_queries(host, &mut target_queries);
        target_queries
    }

    pub(super) fn target_queries_for_source_scope(
        &self,
        host: &DomHost,
        source_scope: &StyleSourceScope,
    ) -> Vec<PendingStyleInvalidationTargetQueries> {
        PendingStyleInvalidationTargetQueries::planned_fallback_root_target(
            host,
            self.runtime_or_source_scope_fallback_target(host, source_scope),
        )
    }
}

impl PendingBoundaryFallbackTarget {
    pub(super) fn new(fallback_target: StyloPlannedFallbackRootInvalidationTarget) -> Self {
        Self { fallback_target }
    }

    pub(super) fn into_target_queries(
        self,
        host: &DomHost,
    ) -> Vec<PendingStyleInvalidationTargetQueries> {
        target_queries_for_planned_fallback_target(host, self.fallback_target)
    }
}

fn target_queries_for_planned_fallback_target(
    host: &DomHost,
    fallback_target: StyloPlannedFallbackRootInvalidationTarget,
) -> Vec<PendingStyleInvalidationTargetQueries> {
    PendingStyleInvalidationTargetQueries::planned_fallback_root_target(host, fallback_target)
}
