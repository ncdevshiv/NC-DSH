use moli_selector::{StyloDomStyleAdapter, StyloStyleSourceScope as StyleSourceScope};

use crate::{
    document_runtime::DomHandle, dom::native::DomHost, protocol_types::EmulatedMediaOverrides,
    style_engine::StyleViewport,
};

#[cfg(test)]
use super::source::adopted::AdoptedStyleSheetSources;
use super::{
    cause::{PendingCauseFallback, PendingStyleInvalidationCause},
    mutation_effect::{
        style_mutation_effects_are_all_attributes, style_mutation_effects_are_all_character_data,
        style_mutation_effects_are_all_slot_assignments,
        style_mutation_effects_are_child_list_structural,
    },
    pending_invalidation::PendingStyleInvalidationWork,
    query::retained_base_query_plan_for_pending_cause,
    request::RetainedSourceDependencyRequestContext,
    snapshot::moli_style_mutation_snapshot_for_pending_cause,
    source_document::DocumentStyleSourceStores,
    target_queries::PendingStyleInvalidationTargetQueries,
};
#[cfg(test)]
use super::{source::linked::LinkedStylesheetSources, source_owner_text::OwnerStyleSheetSources};

pub(super) fn pending_work_for_pending_cause(
    host: &DomHost,
    source_stores: &DocumentStyleSourceStores<'_>,
    dom_adapter: &StyloDomStyleAdapter,
    emulated_media: &EmulatedMediaOverrides,
    viewport: StyleViewport,
    document: DomHandle,
    cause: PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Option<PendingStyleInvalidationWork> {
    let target_queries = target_queries_for_pending_cause_with_linked_and_adopted_sources(
        host,
        source_stores,
        dom_adapter,
        emulated_media,
        viewport,
        document,
        &cause,
        source_scope,
    );
    if target_queries.is_empty() {
        return None;
    }
    Some(PendingStyleInvalidationWork::new(
        cause.work_kind(),
        target_queries,
        cause.pending_merge_class(),
    ))
}

#[cfg(test)]
pub(super) fn target_queries_for_pending_cause_with_document_adopted_sources(
    host: &DomHost,
    linked_stylesheet_sources: &LinkedStylesheetSources,
    document_adopted_sources: &AdoptedStyleSheetSources,
    dom_adapter: &StyloDomStyleAdapter,
    emulated_media: &EmulatedMediaOverrides,
    viewport: StyleViewport,
    document: DomHandle,
    cause: &PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Vec<PendingStyleInvalidationTargetQueries> {
    let owner_sources = OwnerStyleSheetSources::default();
    let source_stores = DocumentStyleSourceStores::borrowed_for_test(
        document,
        linked_stylesheet_sources,
        &owner_sources,
        document_adopted_sources,
    );
    target_queries_for_pending_cause_with_linked_and_adopted_sources(
        host,
        &source_stores,
        dom_adapter,
        emulated_media,
        viewport,
        document,
        cause,
        source_scope,
    )
}

#[cfg(test)]
pub(super) fn target_queries_for_pending_cause_with_adopted_sources(
    host: &DomHost,
    linked_stylesheet_sources: &LinkedStylesheetSources,
    owner_sources: &OwnerStyleSheetSources,
    adopted_sources: &AdoptedStyleSheetSources,
    dom_adapter: &StyloDomStyleAdapter,
    emulated_media: &EmulatedMediaOverrides,
    viewport: StyleViewport,
    document: DomHandle,
    cause: &PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Vec<PendingStyleInvalidationTargetQueries> {
    let source_stores = DocumentStyleSourceStores::borrowed_for_test(
        document,
        linked_stylesheet_sources,
        owner_sources,
        adopted_sources,
    );
    target_queries_for_pending_cause_with_linked_and_adopted_sources(
        host,
        &source_stores,
        dom_adapter,
        emulated_media,
        viewport,
        document,
        cause,
        source_scope,
    )
}

fn target_queries_for_pending_cause_with_linked_and_adopted_sources(
    host: &DomHost,
    source_stores: &DocumentStyleSourceStores<'_>,
    dom_adapter: &StyloDomStyleAdapter,
    emulated_media: &EmulatedMediaOverrides,
    viewport: StyleViewport,
    document: DomHandle,
    cause: &PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Vec<PendingStyleInvalidationTargetQueries> {
    retained_target_queries_for_pending_cause(
        host,
        source_stores,
        dom_adapter,
        emulated_media,
        viewport,
        document,
        cause,
        source_scope,
    )
    .unwrap_or_else(|| fallback_target_queries_for_pending_cause(host, cause, source_scope))
}

fn retained_target_queries_for_pending_cause(
    host: &DomHost,
    source_stores: &DocumentStyleSourceStores<'_>,
    dom_adapter: &StyloDomStyleAdapter,
    emulated_media: &EmulatedMediaOverrides,
    viewport: StyleViewport,
    document: DomHandle,
    cause: &PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Option<Vec<PendingStyleInvalidationTargetQueries>> {
    let mutation_snapshot =
        moli_style_mutation_snapshot_for_pending_cause(host, dom_adapter, cause)
            .unwrap_or_default();
    let base = retained_base_query_plan_for_pending_cause(host, cause, &mutation_snapshot)?;
    let cause_fallback = PendingCauseFallback::from_cause(host, cause);
    let request_context = retained_source_dependency_request_context_for_pending_cause(
        host,
        cause,
        &mutation_snapshot,
    );
    Some(base.target_queries(
        host,
        source_stores,
        emulated_media,
        viewport,
        document,
        &cause_fallback,
        &request_context,
        source_scope,
        &mutation_snapshot,
    ))
}

fn retained_source_dependency_request_context_for_pending_cause<'a>(
    host: &'a DomHost,
    cause: &'a PendingStyleInvalidationCause,
    mutation_snapshot: &'a moli_selector::MoliStyleMutationSnapshot,
) -> RetainedSourceDependencyRequestContext<'a> {
    match cause {
        PendingStyleInvalidationCause::Mutation(effects)
            if style_mutation_effects_are_all_attributes(effects) =>
        {
            RetainedSourceDependencyRequestContext::Attribute
        }
        PendingStyleInvalidationCause::Mutation(effects)
            if style_mutation_effects_are_child_list_structural(effects) =>
        {
            RetainedSourceDependencyRequestContext::ChildList(mutation_snapshot)
        }
        PendingStyleInvalidationCause::Mutation(effects)
            if style_mutation_effects_are_all_character_data(effects) =>
        {
            RetainedSourceDependencyRequestContext::CharacterData { host, effects }
        }
        PendingStyleInvalidationCause::Mutation(effects)
            if style_mutation_effects_are_all_slot_assignments(effects) =>
        {
            RetainedSourceDependencyRequestContext::SlotAssignment(effects)
        }
        PendingStyleInvalidationCause::CustomStateChange { .. } => {
            RetainedSourceDependencyRequestContext::CurrentElement { host }
        }
        _ => RetainedSourceDependencyRequestContext::None,
    }
}

fn fallback_target_queries_for_pending_cause(
    host: &DomHost,
    cause: &PendingStyleInvalidationCause,
    source_scope: &StyleSourceScope,
) -> Vec<PendingStyleInvalidationTargetQueries> {
    PendingCauseFallback::from_cause(host, cause)
        .target_queries_for_source_scope(host, source_scope)
}
