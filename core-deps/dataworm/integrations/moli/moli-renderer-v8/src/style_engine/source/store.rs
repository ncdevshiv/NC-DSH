use std::{
    hash::{Hash, Hasher},
    sync::{Arc as StdArc, LazyLock},
};

use moli_crypto::Sha256Context;
use moli_selector::StyloSourceDependencySummary;

use super::super::{
    source_id::{StyleScopeId, StyleSourceId, StyleSourceKind},
    system::{StyleSystemSourceKey, StyleSystemSourceSetKey},
};
use super::imports::stylesheet_top_level_import_urls;
use super::shared_cache::{SharedStyleSourceContents, shared_style_source_contents};
use crate::document_runtime::DomHandle;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StylesheetFontFaceDescriptor {
    family: StdArc<str>,
    source: StdArc<str>,
}

static EMPTY_STYLESHEET_FONT_FACES: LazyLock<StdArc<[StylesheetFontFaceDescriptor]>> =
    LazyLock::new(|| StdArc::from([]));

#[derive(Clone, Debug)]
pub(crate) struct StyloStylesheetSource {
    contents: StyloStylesheetSourceContents,
    /// Final response URL used as the external stylesheet parser base.
    pub(in crate::style_engine) base_url: StdArc<url::Url>,
    /// Stable URL exposed by the top-level `CSSStyleSheet.href`.
    sheet_url: StdArc<url::Url>,
    origin_clean: bool,
    cache_key: StyleSystemSourceKey,
    source_id: Option<StyleSourceId>,
    adopted_client_id: Option<u64>,
}

#[derive(Clone, Debug)]
enum StyloStylesheetSourceContents {
    Text {
        shared: StdArc<SharedStyleSourceContents>,
    },
    Live {
        stylesheet: style::servo_arc::Arc<style::stylesheets::Stylesheet>,
        id: crate::live_stylesheet::StylesheetId,
        contents_revision: u64,
        cascade_generation: u64,
        derived_state: StdArc<crate::live_stylesheet::LiveStylesheetDerivedState>,
        shared_initial_contents: Option<StdArc<crate::live_stylesheet::SharedStylesheetContents>>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::style_engine) struct StyleSourceMetadata {
    pub(in crate::style_engine) dependency_summary: StyloSourceDependencySummary,
}

/// Immutable stylesheet source produced by processing an inline `<style>` owner.
///
/// The parser base belongs to the processed source, not to a later source-set
/// rebuild. The processing source owns the original owner text for parser-only
/// compatibility extensions without reconstructing parser state from the live
/// document or retaining a second copy on the owner record.
#[derive(Debug)]
pub(crate) struct OwnerStyleSheetSource {
    owner: DomHandle,
    source: StyloStylesheetSource,
    import_urls: StdArc<[url::Url]>,
}

impl StyloStylesheetSource {
    pub(crate) fn new(css_text: String, base_url: url::Url) -> Self {
        let shared = shared_style_source_contents(css_text, base_url);
        let cache_key =
            StyleSystemSourceKey::from_css_fingerprint(shared.css_fingerprint(), shared.base_url());
        let base_url = shared.base_url_handle();
        Self {
            contents: StyloStylesheetSourceContents::Text { shared },
            sheet_url: StdArc::clone(&base_url),
            base_url,
            origin_clean: true,
            cache_key,
            source_id: None,
            adopted_client_id: None,
        }
    }

    pub(crate) fn from_live_stylesheet(
        stylesheet: &crate::live_stylesheet::LiveStylesheetRef,
    ) -> Self {
        let base_url = stylesheet.base_url().clone();
        Self {
            contents: StyloStylesheetSourceContents::Live {
                stylesheet: stylesheet.stylesheet(),
                id: stylesheet.id(),
                contents_revision: stylesheet.contents_revision(),
                cascade_generation: stylesheet.cascade_generation(),
                derived_state: stylesheet.derived_state(),
                shared_initial_contents: stylesheet.shared_initial_contents(),
            },
            base_url: StdArc::new(base_url.clone()),
            sheet_url: StdArc::new(base_url),
            origin_clean: true,
            cache_key: StyleSystemSourceKey::from_live_stylesheet(
                stylesheet.id(),
                stylesheet.cascade_generation(),
            ),
            source_id: None,
            adopted_client_id: None,
        }
    }

    pub(crate) fn with_source_id(mut self, source_id: Option<StyleSourceId>) -> Self {
        self.source_id = source_id;
        self
    }

    pub(in crate::style_engine) fn with_adopted_client_id(mut self, client_id: u64) -> Self {
        self.adopted_client_id = Some(client_id);
        self
    }

    pub(crate) fn with_origin_clean(mut self, origin_clean: bool) -> Self {
        self.origin_clean = origin_clean;
        self
    }

    pub(crate) fn with_sheet_url(mut self, sheet_url: url::Url) -> Self {
        self.sheet_url = StdArc::new(sheet_url);
        self
    }

    pub(crate) fn input_css_text(&self) -> Option<&str> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => Some(shared.css_text()),
            StyloStylesheetSourceContents::Live { .. } => None,
        }
    }

    pub(crate) fn serialized_css_text(&self) -> StdArc<str> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => shared.css_text_handle(),
            StyloStylesheetSourceContents::Live {
                stylesheet,
                contents_revision,
                derived_state,
                ..
            } => derived_state.serialized_css_text(*contents_revision, || {
                moli_css_parse::native_stylesheet_css_text_with_stylo(stylesheet)
            }),
        }
    }

    fn processing_contents(&self) -> Option<&SharedStyleSourceContents> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => Some(shared),
            StyloStylesheetSourceContents::Live { .. } => None,
        }
    }

    pub(crate) fn parsed_stylesheet(
        &self,
    ) -> Option<style::servo_arc::Arc<style::stylesheets::Stylesheet>> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { .. } => None,
            StyloStylesheetSourceContents::Live { stylesheet, .. } => Some(stylesheet.clone()),
        }
    }

    pub(crate) fn live_stylesheet_id(&self) -> Option<crate::live_stylesheet::StylesheetId> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { .. } => None,
            StyloStylesheetSourceContents::Live { id, .. } => Some(*id),
        }
    }

    pub(crate) fn shared_initial_contents(
        &self,
    ) -> Option<StdArc<crate::live_stylesheet::SharedStylesheetContents>> {
        match &self.contents {
            StyloStylesheetSourceContents::Live {
                shared_initial_contents,
                ..
            } => shared_initial_contents.as_ref().map(StdArc::clone),
            StyloStylesheetSourceContents::Text { .. } => None,
        }
    }

    pub(crate) fn base_url(&self) -> &url::Url {
        &self.base_url
    }

    pub(crate) fn sheet_url(&self) -> &url::Url {
        &self.sheet_url
    }

    pub(crate) fn origin_clean(&self) -> bool {
        self.origin_clean
    }

    pub(in crate::style_engine) fn cache_key(&self) -> StyleSystemSourceKey {
        self.cache_key
    }

    pub(in crate::style_engine) fn source_dependency_summary(
        &self,
    ) -> StdArc<StyloSourceDependencySummary> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => shared.source_dependency_summary(),
            StyloStylesheetSourceContents::Live {
                stylesheet,
                contents_revision,
                cascade_generation,
                derived_state,
                ..
            } => derived_state.source_dependency_summary(
                *contents_revision,
                *cascade_generation,
                || {
                    super::super::retained::style_source_metadata_for_stylesheet(stylesheet)
                        .dependency_summary
                },
            ),
        }
    }

    pub(crate) fn font_faces(&self) -> StdArc<[StylesheetFontFaceDescriptor]> {
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => shared.font_faces(),
            StyloStylesheetSourceContents::Live { .. } => {
                StdArc::clone(&EMPTY_STYLESHEET_FONT_FACES)
            }
        }
    }

    pub(in crate::style_engine) fn source_id(&self) -> Option<&StyleSourceId> {
        self.source_id.as_ref()
    }

    pub(crate) fn owner_style_sheet_owner(&self) -> Option<DomHandle> {
        self.source_id.as_ref().and_then(|source_id| {
            if let StyleSourceKind::OwnerStyleSheet { owner } = source_id.kind {
                Some(owner)
            } else {
                None
            }
        })
    }

    pub(in crate::style_engine) fn adopted_client_id(&self) -> Option<u64> {
        self.adopted_client_id
    }

    pub(in crate::style_engine) fn has_same_installation_identity(&self, other: &Self) -> bool {
        match (self.live_stylesheet_id(), other.live_stylesheet_id()) {
            (Some(left), Some(right)) => left == right,
            (None, None) => {
                self.cache_key == other.cache_key
                    && self.base_url == other.base_url
                    && self.sheet_url == other.sheet_url
                    && self.origin_clean == other.origin_clean
            }
            _ => false,
        }
    }

    #[cfg(test)]
    pub(in crate::style_engine) fn shares_source_storage_for_test(&self, other: &Self) -> bool {
        match (&self.contents, &other.contents) {
            (
                StyloStylesheetSourceContents::Text { shared: left },
                StyloStylesheetSourceContents::Text { shared: right },
            ) => StdArc::ptr_eq(left, right),
            (
                StyloStylesheetSourceContents::Live {
                    stylesheet: left_stylesheet,
                    derived_state: left_derived_state,
                    ..
                },
                StyloStylesheetSourceContents::Live {
                    stylesheet: right_stylesheet,
                    derived_state: right_derived_state,
                    ..
                },
            ) => {
                style::servo_arc::Arc::ptr_eq(left_stylesheet, right_stylesheet)
                    && StdArc::ptr_eq(left_derived_state, right_derived_state)
            }
            _ => false,
        }
    }
}

impl PartialEq for StyloStylesheetSource {
    fn eq(&self, other: &Self) -> bool {
        self.base_url == other.base_url
            && self.sheet_url == other.sheet_url
            && self.origin_clean == other.origin_clean
            && self.cache_key == other.cache_key
            && self.source_id == other.source_id
            && self.adopted_client_id == other.adopted_client_id
            && match (&self.contents, &other.contents) {
                (
                    StyloStylesheetSourceContents::Text { shared: left },
                    StyloStylesheetSourceContents::Text { shared: right },
                ) => left == right,
                (
                    StyloStylesheetSourceContents::Live { .. },
                    StyloStylesheetSourceContents::Live { .. },
                ) => true,
                _ => false,
            }
    }
}

impl Eq for StyloStylesheetSource {}

impl Hash for StyloStylesheetSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.base_url.hash(state);
        self.sheet_url.hash(state);
        self.origin_clean.hash(state);
        self.cache_key.hash(state);
        self.source_id.hash(state);
        self.adopted_client_id.hash(state);
        match &self.contents {
            StyloStylesheetSourceContents::Text { shared } => {
                0_u8.hash(state);
                shared.hash(state);
            }
            StyloStylesheetSourceContents::Live { .. } => 1_u8.hash(state),
        }
    }
}

impl StylesheetFontFaceDescriptor {
    pub(super) fn new(family: String, source: String) -> Self {
        Self {
            family: StdArc::from(family),
            source: StdArc::from(source),
        }
    }

    pub(crate) fn family(&self) -> &str {
        self.family.as_ref()
    }

    pub(crate) fn source(&self) -> &str {
        self.source.as_ref()
    }
}

impl OwnerStyleSheetSource {
    pub(crate) fn new(owner: DomHandle, css_text: String, parser_base: url::Url) -> Self {
        let source = StyloStylesheetSource::new(css_text, parser_base);
        let processing_contents = source
            .processing_contents()
            .expect("owner processing source must remain text-backed");
        let import_urls = stylesheet_top_level_import_urls(
            processing_contents.css_text(),
            source.base_url(),
            false,
        )
        .unwrap_or_default();
        Self {
            owner,
            source,
            import_urls: StdArc::from(import_urls),
        }
    }

    pub(in crate::style_engine) fn matches_processing_input(
        &self,
        css_text: &str,
        parser_base: &url::Url,
    ) -> bool {
        self.css_text() == css_text && self.source.base_url() == parser_base
    }

    pub(crate) fn css_text(&self) -> &str {
        self.source
            .processing_contents()
            .map(SharedStyleSourceContents::css_text)
            .expect("owner processing source must remain text-backed")
    }

    pub(crate) fn parser_base(&self) -> &url::Url {
        self.source.base_url()
    }

    pub(in crate::style_engine) fn source(&self) -> &StyloStylesheetSource {
        &self.source
    }

    pub(crate) fn import_urls(&self) -> &[url::Url] {
        self.import_urls.as_ref()
    }

    pub(crate) fn font_faces(&self) -> StdArc<[StylesheetFontFaceDescriptor]> {
        self.source.font_faces()
    }

    pub(crate) fn owner(&self) -> DomHandle {
        self.owner
    }
}

pub(in crate::style_engine) fn stylesheet_sources_cache_key(
    sources: &[StyloStylesheetSource],
) -> StyleSystemSourceSetKey {
    let mut hasher = Sha256Context::new();
    hasher.update(sources.len().to_le_bytes());
    for source in sources {
        hasher.update(source.cache_key().fingerprint);
        update_style_source_identity_hash(&mut hasher, source.source_id());
        match source.adopted_client_id() {
            Some(client_id) => {
                hasher.update([1]);
                hasher.update(client_id.to_le_bytes());
            }
            None => hasher.update([0]),
        }
    }
    StyleSystemSourceSetKey {
        len: sources.len(),
        fingerprint: hasher.finish(),
    }
}

fn update_style_source_identity_hash(
    hasher: &mut Sha256Context,
    source_id: Option<&StyleSourceId>,
) {
    let Some(source_id) = source_id else {
        hasher.update([0]);
        return;
    };
    hasher.update([1]);
    update_style_scope_identity_hash(hasher, source_id.scope_id);
    match &source_id.kind {
        StyleSourceKind::OwnerStyleSheet { owner } => {
            hasher.update([0]);
            hasher.update(owner.index().to_le_bytes());
        }
        StyleSourceKind::LinkedStyleSheet { owner } => {
            hasher.update([1]);
            hasher.update(owner.index().to_le_bytes());
        }
        StyleSourceKind::DocumentAdoptedStyleSheet { index } => {
            hasher.update([2]);
            hasher.update(index.to_le_bytes());
        }
        StyleSourceKind::ShadowRootAdoptedStyleSheet { index } => {
            hasher.update([3]);
            hasher.update(index.to_le_bytes());
        }
    }
}

fn update_style_scope_identity_hash(hasher: &mut Sha256Context, scope_id: StyleScopeId) {
    match scope_id {
        StyleScopeId::Document(document) => {
            hasher.update([0]);
            hasher.update(document.index().to_le_bytes());
        }
        StyleScopeId::ShadowRoot(root) => {
            hasher.update([1]);
            hasher.update(root.index().to_le_bytes());
        }
    }
}
