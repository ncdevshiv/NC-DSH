use moli_crypto::Sha256Context;

use crate::{document_runtime::DomHandle, dom::native::DomHost};

use moli_selector::StyloDomStyleAdapter;
use style::context::QuirksMode;

use super::{
    StyleViewport, StyloComputedStyleInputs, StyloStyleEnvironment,
    cleanup::StyleCacheCleanup,
    retained::build_retained_style_system,
    source::store::{StyloStylesheetSource, stylesheet_sources_cache_key},
    source_dirty::StyleSourceDirtyScopeSnapshot,
    source_document::DocumentStyleSourceStores,
    source_id::StyleSourceId,
    source_lifecycle::{
        StyleSourceDocumentContext, StyleSourceLifecycleOwnerDetailTrace,
        StyleSourceLifecycleOwnerDetailTraceSink, StyleSourceLifecycleReport,
        StyleSourceLifecycleSnapshot, StyleSourceLifecycleSnapshotSink,
    },
    state::{RetainedStyleSystem, StyleDocumentState},
};

pub(super) const DEFAULT_VIEWPORT_WIDTH: f32 = 1024.0;
pub(super) const DEFAULT_VIEWPORT_HEIGHT: f32 = 768.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct StyleSystemCacheKey {
    pub(super) document_url: url::Url,
    pub(super) viewport_width_bits: u32,
    pub(super) viewport_height_bits: u32,
    pub(super) screen_width_bits: u32,
    pub(super) screen_height_bits: u32,
    pub(super) environment: StyloStyleEnvironment,
    pub(super) quirks_mode: QuirksMode,
    pub(super) script_custom_property_registrations: StyleSystemScriptCustomPropertyKey,
    pub(super) document_stylesheet_sources: StyleSystemSourceSetKey,
    pub(super) shadow_stylesheet_sources: Vec<(DomHandle, StyleSystemSourceSetKey)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct StyleSystemSourceSetKey {
    pub(super) len: usize,
    pub(super) fingerprint: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct StyleSystemSourceKey {
    pub(super) fingerprint: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct StyleSystemScriptCustomPropertyKey {
    len: usize,
    fingerprint: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StyleSourceInputTrace {
    pub(super) document_stylesheet_source_count: usize,
    pub(super) document_source_ids: Vec<Option<StyleSourceId>>,
    pub(super) shadow_stylesheet_sources: Vec<StyleSourceInputShadowRootTrace>,
    pub(super) script_custom_property_registration_count: usize,
    pub(super) script_custom_property_base_url: url::Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StyleSourceInputShadowRootTrace {
    pub(super) root: DomHandle,
    pub(super) source_count: usize,
    pub(super) source_ids: Vec<Option<StyleSourceId>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StyleSystemCacheKeyMismatchTrace {
    pub(super) document_url_changed: bool,
    pub(super) previous_document_url: url::Url,
    pub(super) next_document_url: url::Url,
    pub(super) viewport_changed: bool,
    pub(super) previous_viewport_width_bits: u32,
    pub(super) next_viewport_width_bits: u32,
    pub(super) previous_viewport_height_bits: u32,
    pub(super) next_viewport_height_bits: u32,
    pub(super) screen_changed: bool,
    pub(super) previous_screen_width_bits: u32,
    pub(super) next_screen_width_bits: u32,
    pub(super) previous_screen_height_bits: u32,
    pub(super) next_screen_height_bits: u32,
    pub(super) environment_changed: bool,
    pub(super) quirks_mode_changed: bool,
    pub(super) custom_property_registrations_changed: bool,
    pub(super) previous_custom_property_registrations: StyleSystemScriptCustomPropertyKey,
    pub(super) next_custom_property_registrations: StyleSystemScriptCustomPropertyKey,
    pub(super) document_stylesheet_sources_changed: bool,
    pub(super) previous_document_stylesheet_sources: StyleSystemSourceSetKey,
    pub(super) next_document_stylesheet_sources: StyleSystemSourceSetKey,
    pub(super) shadow_root_list_changed: bool,
    pub(super) previous_shadow_roots: Vec<DomHandle>,
    pub(super) next_shadow_roots: Vec<DomHandle>,
    pub(super) added_shadow_roots: Vec<DomHandle>,
    pub(super) removed_shadow_roots: Vec<DomHandle>,
    pub(super) shadow_stylesheet_sources_changed: bool,
    pub(super) previous_shadow_stylesheet_sources: Vec<(DomHandle, StyleSystemSourceSetKey)>,
    pub(super) next_shadow_stylesheet_sources: Vec<(DomHandle, StyleSystemSourceSetKey)>,
    pub(super) changed_shadow_source_roots: Vec<DomHandle>,
}

impl StyleSystemCacheKey {
    pub(super) fn new(
        document_url: &url::Url,
        inputs: &StyloComputedStyleInputs,
        viewport: impl Into<StyleViewport>,
    ) -> Self {
        let viewport = viewport.into();
        let mut document_url = document_url.clone();
        document_url.set_fragment(None);
        let viewport_width_bits = style_dimension_bits(viewport.width, DEFAULT_VIEWPORT_WIDTH);
        let viewport_height_bits = style_dimension_bits(viewport.height, DEFAULT_VIEWPORT_HEIGHT);
        Self {
            document_url,
            viewport_width_bits,
            viewport_height_bits,
            screen_width_bits: style_dimension_bits(
                viewport.screen_width,
                f32::from_bits(viewport_width_bits),
            ),
            screen_height_bits: style_dimension_bits(
                viewport.screen_height,
                f32::from_bits(viewport_height_bits),
            ),
            environment: inputs.environment,
            quirks_mode: inputs.quirks_mode,
            script_custom_property_registrations: script_custom_property_registrations_cache_key(
                inputs,
            ),
            document_stylesheet_sources: stylesheet_sources_cache_key(
                &inputs.document_stylesheet_sources,
            ),
            shadow_stylesheet_sources: inputs
                .shadow_stylesheet_sources
                .iter()
                .map(|(root, sources)| (*root, stylesheet_sources_cache_key(sources)))
                .collect(),
        }
    }

    pub(super) fn mismatch_trace(&self, next: &Self) -> StyleSystemCacheKeyMismatchTrace {
        let previous_shadow_roots = shadow_roots_for_trace(&self.shadow_stylesheet_sources);
        let next_shadow_roots = shadow_roots_for_trace(&next.shadow_stylesheet_sources);
        StyleSystemCacheKeyMismatchTrace {
            document_url_changed: self.document_url != next.document_url,
            previous_document_url: self.document_url.clone(),
            next_document_url: next.document_url.clone(),
            viewport_changed: self.viewport_width_bits != next.viewport_width_bits
                || self.viewport_height_bits != next.viewport_height_bits,
            previous_viewport_width_bits: self.viewport_width_bits,
            next_viewport_width_bits: next.viewport_width_bits,
            previous_viewport_height_bits: self.viewport_height_bits,
            next_viewport_height_bits: next.viewport_height_bits,
            screen_changed: self.screen_width_bits != next.screen_width_bits
                || self.screen_height_bits != next.screen_height_bits,
            previous_screen_width_bits: self.screen_width_bits,
            next_screen_width_bits: next.screen_width_bits,
            previous_screen_height_bits: self.screen_height_bits,
            next_screen_height_bits: next.screen_height_bits,
            environment_changed: self.environment != next.environment,
            quirks_mode_changed: self.quirks_mode != next.quirks_mode,
            custom_property_registrations_changed: self.script_custom_property_registrations
                != next.script_custom_property_registrations,
            previous_custom_property_registrations: self
                .script_custom_property_registrations
                .clone(),
            next_custom_property_registrations: next.script_custom_property_registrations.clone(),
            document_stylesheet_sources_changed: self.document_stylesheet_sources
                != next.document_stylesheet_sources,
            previous_document_stylesheet_sources: self.document_stylesheet_sources,
            next_document_stylesheet_sources: next.document_stylesheet_sources,
            shadow_root_list_changed: previous_shadow_roots != next_shadow_roots,
            added_shadow_roots: shadow_roots_added_for_trace(
                &previous_shadow_roots,
                &next_shadow_roots,
            ),
            removed_shadow_roots: shadow_roots_added_for_trace(
                &next_shadow_roots,
                &previous_shadow_roots,
            ),
            previous_shadow_roots,
            next_shadow_roots,
            shadow_stylesheet_sources_changed: self.shadow_stylesheet_sources
                != next.shadow_stylesheet_sources,
            previous_shadow_stylesheet_sources: self.shadow_stylesheet_sources.clone(),
            next_shadow_stylesheet_sources: next.shadow_stylesheet_sources.clone(),
            changed_shadow_source_roots: changed_shadow_source_roots_for_trace(
                &self.shadow_stylesheet_sources,
                &next.shadow_stylesheet_sources,
            ),
        }
    }
}

impl StyleSystemCacheKeyMismatchTrace {
    fn is_only_scoped_source_change(&self, roots: &[DomHandle], has_document_scope: bool) -> bool {
        !self.document_url_changed
            && !self.viewport_changed
            && !self.screen_changed
            && !self.environment_changed
            && !self.quirks_mode_changed
            && !self.custom_property_registrations_changed
            && (self.document_stylesheet_sources_changed
                || self.shadow_root_list_changed
                || self.shadow_stylesheet_sources_changed)
            && (!self.document_stylesheet_sources_changed || has_document_scope)
            && (!self.shadow_stylesheet_sources_changed
                || self.shadow_source_scope_changed_roots_covered_by(roots))
            && self.shadow_root_list_change_covered_by(roots)
    }

    fn shadow_source_scope_changed_roots_covered_by(&self, roots: &[DomHandle]) -> bool {
        let mut changed_root_count = 0;
        for root in self
            .changed_shadow_source_roots
            .iter()
            .chain(self.added_shadow_roots.iter())
            .chain(self.removed_shadow_roots.iter())
        {
            changed_root_count += 1;
            if !roots.contains(root) {
                return false;
            }
        }
        changed_root_count > 0
    }

    fn shadow_root_list_change_covered_by(&self, roots: &[DomHandle]) -> bool {
        if !self.shadow_root_list_changed {
            return true;
        }
        let mut changed_root_count = 0;
        for root in self
            .added_shadow_roots
            .iter()
            .chain(self.removed_shadow_roots.iter())
        {
            changed_root_count += 1;
            if !roots.contains(root) {
                return false;
            }
        }
        changed_root_count > 0
    }
}

fn shadow_roots_for_trace(
    shadow_sources: &[(DomHandle, StyleSystemSourceSetKey)],
) -> Vec<DomHandle> {
    shadow_sources.iter().map(|(root, _)| *root).collect()
}

fn shadow_roots_added_for_trace(previous: &[DomHandle], next: &[DomHandle]) -> Vec<DomHandle> {
    next.iter()
        .copied()
        .filter(|root| !previous.contains(root))
        .collect()
}

fn changed_shadow_source_roots_for_trace(
    previous: &[(DomHandle, StyleSystemSourceSetKey)],
    next: &[(DomHandle, StyleSystemSourceSetKey)],
) -> Vec<DomHandle> {
    next.iter()
        .filter_map(|(root, next_key)| {
            previous
                .iter()
                .find(|(previous_root, _)| previous_root == root)
                .and_then(|(_, previous_key)| (previous_key != next_key).then_some(*root))
        })
        .collect()
}

fn script_custom_property_registrations_cache_key(
    inputs: &StyloComputedStyleInputs,
) -> StyleSystemScriptCustomPropertyKey {
    let mut hasher = Sha256Context::new();
    let mut base_url = inputs.script_custom_property_base_url.clone();
    base_url.set_fragment(None);
    hasher.update(base_url.as_str().len().to_le_bytes());
    hasher.update(base_url.as_str().as_bytes());
    hasher.update(
        inputs
            .script_custom_property_registrations
            .len()
            .to_le_bytes(),
    );
    for registration in &inputs.script_custom_property_registrations {
        update_string_hash(&mut hasher, &registration.name);
        update_string_hash(&mut hasher, &registration.syntax);
        hasher.update([registration.inherits as u8]);
        match registration.initial_value.as_deref() {
            Some(initial_value) => {
                hasher.update([1]);
                update_string_hash(&mut hasher, initial_value);
            }
            None => hasher.update([0]),
        }
    }
    StyleSystemScriptCustomPropertyKey {
        len: inputs.script_custom_property_registrations.len(),
        fingerprint: hasher.finish(),
    }
}

fn update_string_hash(hasher: &mut Sha256Context, value: &str) {
    hasher.update(value.len().to_le_bytes());
    hasher.update(value.as_bytes());
}

impl StyleSystemSourceKey {
    pub(super) fn css_fingerprint(css_text: &str) -> [u8; 32] {
        let mut hasher = Sha256Context::new();
        let css_text = css_text.as_bytes();
        hasher.update(css_text.len().to_le_bytes());
        hasher.update(css_text);
        hasher.finish()
    }

    pub(super) fn from_css_fingerprint(css_fingerprint: [u8; 32], base_url: &url::Url) -> Self {
        let mut hasher = Sha256Context::new();
        hasher.update(css_fingerprint);
        let mut base_url = base_url.clone();
        base_url.set_fragment(None);
        let base_url = base_url.as_str().as_bytes();
        hasher.update(base_url.len().to_le_bytes());
        hasher.update(base_url);
        Self {
            fingerprint: hasher.finish(),
        }
    }

    pub(super) fn from_live_stylesheet(
        stylesheet_id: crate::live_stylesheet::StylesheetId,
        cascade_generation: u64,
    ) -> Self {
        let mut hasher = Sha256Context::new();
        hasher.update(b"live-stylesheet");
        hasher.update(stylesheet_id.get().to_le_bytes());
        hasher.update(cascade_generation.to_le_bytes());
        Self {
            fingerprint: hasher.finish(),
        }
    }
}

pub(super) fn ensure_retained_style_system(
    host: &DomHost,
    dom_adapter: &StyloDomStyleAdapter,
    document_state: &StyleDocumentState,
    source_stores: &DocumentStyleSourceStores<'_>,
    document_context: StyleSourceDocumentContext<'_>,
    retained_document: DomHandle,
    cache_cleanup: StyleCacheCleanup<'_>,
    key: &StyleSystemCacheKey,
    inputs: &StyloComputedStyleInputs,
) {
    if document_state.retained_style_system_matches(key) {
        return;
    }
    let trace_enabled = moli_trace::style_invalidation_trace_enabled();
    let key_mismatch_trace =
        document_state.try_with_retained_style_system(|retained| retained.key.mismatch_trace(key));
    let trace_documents = trace_enabled.then(|| vec![retained_document]);

    let source_dirty_scope = document_state.source_dirty_scope_snapshot();
    let scoped_roots_vec = source_dirty_scope.scoped_roots_vec();
    if source_dirty_scope.has_only_scoped_source_records()
        && key_mismatch_trace.as_ref().is_some_and(|trace| {
            trace.is_only_scoped_source_change(
                &scoped_roots_vec,
                source_dirty_scope.contains_document_scope(retained_document),
            )
        })
    {
        if !source_dirty_scope.cache_cleanup_covers(
            scoped_roots_vec.iter().copied(),
            cache_cleanup.computed_style_cache_write_generation(),
        ) {
            cache_cleanup.clear_for_scoped_retained_style_system_rebuild(
                host,
                scoped_roots_vec.iter().copied(),
            );
        }
        document_state.clear_source_dirty_scopes();
    } else {
        cache_cleanup.clear_for_retained_style_system_rebuild(host);
        document_state.clear_source_dirty_scopes();
    }

    let source_lifecycle = source_stores.source_lifecycle_report(host, document_context);
    let retained_source_records =
        source_stores.retained_source_records_for_lifecycle(host, &source_lifecycle);
    let shared_lock = dom_adapter.shared_lock().clone();
    let retained = build_retained_style_system(
        host,
        key.clone(),
        inputs,
        &shared_lock,
        &retained_source_records,
    );
    if trace_enabled {
        trace_retained_style_system_rebuild(
            inputs,
            &source_lifecycle,
            &source_dirty_scope,
            document_state.source_set_generation(),
            &retained,
            key_mismatch_trace.as_ref(),
            trace_documents.as_deref().unwrap_or_default(),
        );
    }
    document_state.set_retained_style_system(retained);
}

fn trace_retained_style_system_rebuild(
    inputs: &StyloComputedStyleInputs,
    source_lifecycle: &StyleSourceLifecycleReport,
    source_dirty_scope: &StyleSourceDirtyScopeSnapshot,
    source_set_generation: u64,
    retained: &RetainedStyleSystem,
    key_mismatch: Option<&StyleSystemCacheKeyMismatchTrace>,
    document_context_documents: &[DomHandle],
) {
    let mut lifecycle_snapshot = RetainedStyleSystemRebuildLifecycleSnapshot::default();
    source_lifecycle.record_snapshot_into(&mut lifecycle_snapshot);
    source_lifecycle.record_owner_detail_trace_into(&mut lifecycle_snapshot);
    let source_input = style_source_input_trace(inputs);
    tracing::info!(
        document_url = %retained.key.document_url,
        document_stylesheet_input_count = inputs.document_stylesheet_sources.len(),
        shadow_stylesheet_input_count = inputs.shadow_stylesheet_sources.len(),
        retained_shadow_cascade_count = retained.shadow_cascade_data.len(),
        retained_source_cascade_data_count = retained.source_cascade_data.len(),
        document_context_documents = ?document_context_documents,
        source_set_generation = source_set_generation,
        source_dirty_ids = ?source_dirty_scope.source_ids_vec(),
        source_dirty_scope_ids = ?source_dirty_scope.scope_ids_vec(),
        source_dirty_roots = ?source_dirty_scope.scoped_roots_vec(),
        source_dirty_reasons = ?source_dirty_scope.reasons_vec(),
        source_dirty_records = ?source_dirty_scope.records_vec(),
        source_input = ?source_input,
        key_mismatch = ?key_mismatch,
        source_lifecycle = ?lifecycle_snapshot.snapshot,
        source_lifecycle_owner_details = ?lifecycle_snapshot.owner_details,
        "retained style system rebuild summary"
    );
}

fn style_source_input_trace(inputs: &StyloComputedStyleInputs) -> StyleSourceInputTrace {
    StyleSourceInputTrace {
        document_stylesheet_source_count: inputs.document_stylesheet_sources.len(),
        document_source_ids: style_source_ids_for_trace(&inputs.document_stylesheet_sources),
        shadow_stylesheet_sources: inputs
            .shadow_stylesheet_sources
            .iter()
            .map(|(root, sources)| StyleSourceInputShadowRootTrace {
                root: *root,
                source_count: sources.len(),
                source_ids: style_source_ids_for_trace(sources),
            })
            .collect(),
        script_custom_property_registration_count: inputs
            .script_custom_property_registrations
            .len(),
        script_custom_property_base_url: inputs.script_custom_property_base_url.clone(),
    }
}

fn style_source_ids_for_trace(sources: &[StyloStylesheetSource]) -> Vec<Option<StyleSourceId>> {
    sources
        .iter()
        .map(|source| source.source_id().cloned())
        .collect()
}

#[cfg(test)]
pub(super) fn style_source_input_trace_for_test(
    inputs: &StyloComputedStyleInputs,
) -> StyleSourceInputTrace {
    style_source_input_trace(inputs)
}

#[derive(Default)]
struct RetainedStyleSystemRebuildLifecycleSnapshot {
    snapshot: StyleSourceLifecycleSnapshot,
    owner_details: Vec<StyleSourceLifecycleOwnerDetailTrace>,
}

impl StyleSourceLifecycleSnapshotSink for RetainedStyleSystemRebuildLifecycleSnapshot {
    fn record_source_lifecycle_snapshot(&mut self, snapshot: StyleSourceLifecycleSnapshot) {
        self.snapshot = snapshot;
    }
}

impl StyleSourceLifecycleOwnerDetailTraceSink for RetainedStyleSystemRebuildLifecycleSnapshot {
    fn record_source_lifecycle_owner_detail_trace(
        &mut self,
        trace: StyleSourceLifecycleOwnerDetailTrace,
    ) {
        self.owner_details.push(trace);
    }
}

fn style_dimension_bits(value: Option<f64>, fallback: f32) -> u32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as f32)
        .unwrap_or(fallback)
        .to_bits()
}
