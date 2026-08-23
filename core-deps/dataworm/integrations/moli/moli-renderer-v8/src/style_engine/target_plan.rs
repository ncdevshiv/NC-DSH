#[cfg(test)]
use moli_selector::stylo_source_dependency_invalidation_batch_plan;
use moli_selector::{
    StyloPlannedFallbackRootInvalidationTarget, StyloPlannedSourceDependencyInvalidation,
    StyloSourceDependencyBoundaryRoots, StyloSourceDependencyInvalidationBatchPlan,
    StyloSourceDependencyInvalidationBatchPlanSink, StyloSourceDependencyInvalidationRequest,
};

use crate::document_runtime::DomHandle;
use crate::dom::native::DomHost;

use super::{
    cause::PendingCauseFallback, fallback::PendingBoundaryFallbackTarget,
    source_record::MatchingStyleDependencySource,
    target_queries::PendingStyleInvalidationTargetQueries,
};

pub(super) struct PendingSourceDependencyTargetPlan {
    target_queries: Vec<PendingStyleInvalidationTargetQueries>,
    boundary_fallback: Option<PendingBoundaryFallbackTarget>,
}

struct PendingSourceDependencyTargetPlanBuilder<'sources> {
    matching_sources: &'sources [MatchingStyleDependencySource],
    target_plan: PendingSourceDependencyTargetPlan,
}

pub(super) enum SourceDependencyTargetPlanResolution {
    Finalized(Vec<PendingStyleInvalidationTargetQueries>),
    PendingBaseRoots(Vec<PendingStyleInvalidationTargetQueries>),
}

pub(super) trait SourceDependencyCauseFallbackPlan {
    fn source_dependency_batch_plan(
        self,
        host: &DomHost,
        matching_sources: &[MatchingStyleDependencySource],
        requests: &[StyloSourceDependencyInvalidationRequest<'_>],
        boundary_roots: StyloSourceDependencyBoundaryRoots<'_>,
    ) -> StyloSourceDependencyInvalidationBatchPlan;
}

impl SourceDependencyCauseFallbackPlan for &PendingCauseFallback {
    fn source_dependency_batch_plan(
        self,
        host: &DomHost,
        matching_sources: &[MatchingStyleDependencySource],
        requests: &[StyloSourceDependencyInvalidationRequest<'_>],
        boundary_roots: StyloSourceDependencyBoundaryRoots<'_>,
    ) -> StyloSourceDependencyInvalidationBatchPlan {
        PendingCauseFallback::source_dependency_batch_plan(
            self,
            host,
            matching_sources,
            requests,
            boundary_roots,
        )
    }
}

#[cfg(test)]
impl<const N: usize> SourceDependencyCauseFallbackPlan for &[DomHandle; N] {
    fn source_dependency_batch_plan(
        self,
        host: &DomHost,
        matching_sources: &[MatchingStyleDependencySource],
        requests: &[StyloSourceDependencyInvalidationRequest<'_>],
        boundary_roots: StyloSourceDependencyBoundaryRoots<'_>,
    ) -> StyloSourceDependencyInvalidationBatchPlan {
        let batch_sources = matching_sources
            .iter()
            .map(|source| source.stylo_batch_source(self.as_slice()))
            .collect::<Vec<_>>();
        stylo_source_dependency_invalidation_batch_plan(
            host,
            &batch_sources,
            requests,
            boundary_roots,
        )
    }
}

impl PendingSourceDependencyTargetPlan {
    #[cfg(test)]
    pub(super) fn target_queries(&self) -> &[PendingStyleInvalidationTargetQueries] {
        &self.target_queries
    }

    #[cfg(test)]
    pub(super) fn boundary_fallback(&self) -> Option<&PendingBoundaryFallbackTarget> {
        self.boundary_fallback.as_ref()
    }

    pub(super) fn into_target_queries(
        self,
        host: &DomHost,
        cause_fallback: &PendingCauseFallback,
    ) -> SourceDependencyTargetPlanResolution {
        let Self {
            mut target_queries,
            boundary_fallback,
        } = self;
        if target_queries.is_empty()
            && let Some(boundary_fallback) = boundary_fallback
        {
            target_queries = boundary_fallback.into_target_queries(host);
            cause_fallback
                .merge_cause_roots_into_source_dependency_target_queries(host, &mut target_queries);
            return SourceDependencyTargetPlanResolution::Finalized(target_queries);
        }
        if let Some(boundary_fallback) = boundary_fallback {
            let mut boundary_targets = boundary_fallback.into_target_queries(host);
            cause_fallback.merge_cause_roots_into_source_dependency_target_queries(
                host,
                &mut boundary_targets,
            );
            target_queries.extend(boundary_targets);
        }
        SourceDependencyTargetPlanResolution::PendingBaseRoots(target_queries)
    }
}

pub(super) fn pending_target_queries_for_source_dependency_requests(
    host: &DomHost,
    matching_sources: &[MatchingStyleDependencySource],
    requests: &[StyloSourceDependencyInvalidationRequest<'_>],
    cause_fallback: impl SourceDependencyCauseFallbackPlan,
    boundary_roots: StyloSourceDependencyBoundaryRoots<'_>,
) -> PendingSourceDependencyTargetPlan {
    let plan = cause_fallback.source_dependency_batch_plan(
        host,
        matching_sources,
        requests,
        boundary_roots,
    );
    pending_target_queries_for_source_dependency_plan(matching_sources, plan)
}

pub(super) fn pending_target_queries_for_source_dependency_plan(
    matching_sources: &[MatchingStyleDependencySource],
    plan: StyloSourceDependencyInvalidationBatchPlan,
) -> PendingSourceDependencyTargetPlan {
    PendingSourceDependencyTargetPlanBuilder::build(matching_sources, plan)
}

impl<'sources> PendingSourceDependencyTargetPlanBuilder<'sources> {
    fn build(
        matching_sources: &'sources [MatchingStyleDependencySource],
        plan: StyloSourceDependencyInvalidationBatchPlan,
    ) -> PendingSourceDependencyTargetPlan {
        let mut builder = Self {
            matching_sources,
            target_plan: PendingSourceDependencyTargetPlan {
                target_queries: Vec::new(),
                boundary_fallback: None,
            },
        };
        plan.drain_into(&mut builder);
        builder.target_plan
    }
}

impl StyloSourceDependencyInvalidationBatchPlanSink<DomHandle>
    for PendingSourceDependencyTargetPlanBuilder<'_>
{
    fn set_source_dependency_batch_work(
        &mut self,
        sources: Vec<StyloPlannedSourceDependencyInvalidation>,
        boundary_fallback: Option<StyloPlannedFallbackRootInvalidationTarget>,
    ) {
        self.target_plan.target_queries = sources
            .into_iter()
            .map(|planned| self.pending_target_query_for_planned_source(planned))
            .collect();
        self.target_plan.boundary_fallback =
            boundary_fallback.map(PendingBoundaryFallbackTarget::new);
    }

    fn set_source_dependency_batch_requires_source_fallback(
        &mut self,
        source: StyloPlannedSourceDependencyInvalidation,
    ) {
        self.target_plan.target_queries =
            vec![self.pending_target_query_for_planned_source(source)];
        self.target_plan.boundary_fallback = None;
    }
}

impl PendingSourceDependencyTargetPlanBuilder<'_> {
    fn pending_target_query_for_planned_source(
        &self,
        planned: StyloPlannedSourceDependencyInvalidation,
    ) -> PendingStyleInvalidationTargetQueries {
        let source_targets = self
            .matching_sources
            .iter()
            .map(|source| source.target().clone())
            .collect::<Vec<_>>();
        PendingStyleInvalidationTargetQueries::planned_source_dependency(&source_targets, planned)
    }
}
