use euclid::{Scale, Size2D};
use moli_selector::StyloSourceDependencySummary;
use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use std::sync::{Arc as StdArc, LazyLock};
use style::{
    author_styles::AuthorStyles,
    context::QuirksMode,
    device::{Device, servo::FontMetricsProvider},
    font_face::FontFaceRule,
    font_metrics::FontMetrics,
    media_queries::MediaList,
    properties::{ComputedValues, style_structs::Font},
    servo::media_features::PointerCapabilities,
    servo_arc::Arc as ServoArc,
    shared_lock::{Locked, SharedRwLock, StylesheetGuards, ToCssWithGuard},
    stylesheets::{
        AllowImportRules, CssRule, CustomMediaMap, DocumentStyleSheet, Origin, Stylesheet,
        StylesheetInDocument, UrlExtraData, scope_rule::ImplicitScopeRoot,
    },
    stylist::{CascadeData, Stylist},
    values::computed::{
        CSSPixelLength, Length,
        font::{GenericFontFamily, SingleFontFamily},
    },
    values::specified::font::QueryFontMetricsFlags,
};
use style_traits::{CSSPixel, CssWriter, DevicePixel, ToCss};

use crate::dom::native::DomHost;

use super::{
    StyloComputedStyleInputs, StyloStyleEnvironment,
    source::store::{StyleSourceMetadata, StyloStylesheetSource},
    source_id::{StyleSourceId, StyleSourceKind},
    source_record::RetainedStylesheetSourceRecord,
    state::RetainedStyleSystem,
    system::{DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH, StyleSystemCacheKey},
    ua::HTML_STYLESHEET as MOLI_UA_STYLESHEET,
};

static MOLI_UA_SOURCE_METADATA: LazyLock<StyleSourceMetadata> = LazyLock::new(|| {
    style_source_metadata_for_css_text_with_origin(
        MOLI_UA_STYLESHEET,
        &url::Url::parse("about:blank").expect("valid built-in stylesheet base URL"),
        Origin::UserAgent,
    )
});

static MOLI_UA_SOURCE_DEPENDENCY_SUMMARY: LazyLock<StdArc<StyloSourceDependencySummary>> =
    LazyLock::new(|| StdArc::new(MOLI_UA_SOURCE_METADATA.dependency_summary.clone()));

#[cfg(test)]
thread_local! {
    static AUTHOR_SOURCE_TEXT_PARSE_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
pub(crate) fn reset_author_source_text_parse_count_for_test() {
    AUTHOR_SOURCE_TEXT_PARSE_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn author_source_text_parse_count_for_test() -> usize {
    AUTHOR_SOURCE_TEXT_PARSE_COUNT.with(std::cell::Cell::get)
}

#[derive(Debug)]
struct HeadlessFontMetricsProvider;

pub(super) fn build_retained_style_system(
    host: &DomHost,
    key: StyleSystemCacheKey,
    inputs: &StyloComputedStyleInputs,
    shared_lock: &SharedRwLock,
    retained_source_records: &[RetainedStylesheetSourceRecord<'_>],
) -> RetainedStyleSystem {
    let mut stylist = new_stylist_with_viewport_bits(
        key.viewport_width_bits,
        key.viewport_height_bits,
        key.screen_width_bits,
        key.screen_height_bits,
        key.environment,
        key.quirks_mode,
    );
    register_script_custom_properties(&mut stylist, inputs);
    append_stylesheet_to_stylist(
        &mut stylist,
        shared_lock,
        MOLI_UA_STYLESHEET,
        &key.document_url,
        Origin::UserAgent,
        key.quirks_mode,
    );
    for source in &inputs.document_stylesheet_sources {
        append_author_stylesheet_source_to_stylist(
            &mut stylist,
            shared_lock,
            host,
            source,
            key.quirks_mode,
        );
    }
    let mut sources_by_id = HashMap::<StyleSourceId, Vec<StyloStylesheetSource>>::new();
    for source in &inputs.document_stylesheet_sources {
        let Some(source_id) = source.source_id().cloned() else {
            continue;
        };
        sources_by_id
            .entry(source_id)
            .or_default()
            .push(source.clone());
    }
    let mut shadow_cascade_data = Vec::new();
    for (root, sources) in &inputs.shadow_stylesheet_sources {
        let cascade_data =
            build_author_cascade_data(host, &mut stylist, shared_lock, sources, key.quirks_mode);
        shadow_cascade_data.push((*root, cascade_data));
        for source in sources {
            let Some(source_id) = source.source_id().cloned() else {
                continue;
            };
            sources_by_id
                .entry(source_id)
                .or_default()
                .push(source.clone());
        }
    }
    let mut source_cascade_data =
        HashMap::with_capacity(retained_source_records.len().max(sources_by_id.len()));
    for (source_id, sources) in sources_by_id {
        source_cascade_data.insert(
            source_id,
            build_author_cascade_data(host, &mut stylist, shared_lock, &sources, key.quirks_mode),
        );
    }
    for record in retained_source_records {
        if source_cascade_data.contains_key(record.id()) {
            continue;
        }
        let source_id = record.id().clone();
        let source = record.to_stylo_source();
        source_cascade_data.insert(
            source_id,
            build_author_cascade_data(
                host,
                &mut stylist,
                shared_lock,
                std::slice::from_ref(&source),
                key.quirks_mode,
            ),
        );
    }
    let guard = shared_lock.read();
    stylist.flush(&StylesheetGuards::same(&guard));
    let user_agent_cascade_data = ServoArc::new(
        stylist
            .cascade_data()
            .borrow_for_origin(Origin::UserAgent)
            .clone(),
    );

    RetainedStyleSystem {
        key,
        stylist,
        user_agent_cascade_data,
        shadow_cascade_data,
        source_cascade_data,
    }
}

fn register_script_custom_properties(stylist: &mut Stylist, inputs: &StyloComputedStyleInputs) {
    let url_data = UrlExtraData::from(inputs.script_custom_property_base_url.clone());
    for registration in &inputs.script_custom_property_registrations {
        let _ = stylist.register_custom_property(
            &url_data,
            &registration.name,
            &registration.syntax,
            registration.inherits,
            registration.initial_value.as_deref(),
        );
    }
}

pub(super) fn style_source_metadata_for_css_text(
    css_text: &str,
    base_url: &url::Url,
) -> StyleSourceMetadata {
    style_source_metadata_for_css_text_with_origin(css_text, base_url, Origin::Author)
}

pub(in crate::style_engine) fn style_source_metadata_for_stylesheet(
    stylesheet: &ServoArc<Stylesheet>,
) -> StyleSourceMetadata {
    let guard = stylesheet.shared_lock.read();
    let quirks_mode = stylesheet.contents.read_with(&guard).quirks_mode;
    let stylist = new_stylist_with_viewport_bits(
        DEFAULT_VIEWPORT_WIDTH.to_bits(),
        crate::style_engine::system::DEFAULT_VIEWPORT_HEIGHT.to_bits(),
        DEFAULT_VIEWPORT_WIDTH.to_bits(),
        crate::style_engine::system::DEFAULT_VIEWPORT_HEIGHT.to_bits(),
        StyloStyleEnvironment::default(),
        quirks_mode,
    );
    let mut cascade_data = CascadeData::new();
    let document_stylesheet = DocumentStyleSheet::new(stylesheet.clone());
    if cascade_data
        .add_stylesheet_for_moli_source_metadata(
            stylist.device(),
            quirks_mode,
            &document_stylesheet,
            0,
            &guard,
        )
        .is_err()
    {
        return StyleSourceMetadata::default();
    }
    style_source_metadata_from_cascade_data(&cascade_data)
}

#[derive(Clone, Debug)]
pub(crate) struct StylesheetFontFaceRuleProjection {
    pub(crate) rule_identity: u64,
    pub(crate) rule_fingerprint: String,
    pub(crate) descriptor: moli_css_parse::CssFontFace,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StylesheetFontFaceProjection {
    pub(crate) all_rules: Vec<StylesheetFontFaceRuleProjection>,
    pub(crate) effective_rule_identities: HashSet<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeStylesheetFontFaceRuleProjection {
    pub(crate) rule: ServoArc<Locked<FontFaceRule>>,
    pub(crate) rule_fingerprint: String,
    pub(crate) descriptor: moli_css_parse::CssFontFace,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NativeStylesheetFontFaceProjection {
    pub(crate) all_rules: Vec<NativeStylesheetFontFaceRuleProjection>,
    pub(crate) effective_rule_addresses: HashSet<usize>,
}

pub(crate) fn native_font_face_projection_for_stylesheet(
    stylesheet: &ServoArc<Stylesheet>,
    environment: StyloStyleEnvironment,
    viewport: super::StyleViewport,
) -> NativeStylesheetFontFaceProjection {
    let guard = stylesheet.shared_lock.read();
    let quirks_mode = stylesheet.contents.read_with(&guard).quirks_mode;
    let viewport_width = viewport.width.unwrap_or(DEFAULT_VIEWPORT_WIDTH as f64) as f32;
    let viewport_height = viewport.height.unwrap_or(DEFAULT_VIEWPORT_HEIGHT as f64) as f32;
    let screen_width = viewport.screen_width.unwrap_or(f64::from(viewport_width)) as f32;
    let screen_height = viewport.screen_height.unwrap_or(f64::from(viewport_height)) as f32;
    let stylist = new_stylist_with_viewport_bits(
        viewport_width.to_bits(),
        viewport_height.to_bits(),
        screen_width.to_bits(),
        screen_height.to_bits(),
        environment,
        quirks_mode,
    );

    let mut projection = NativeStylesheetFontFaceProjection::default();
    let contents = stylesheet.contents(&guard);
    collect_font_face_rule_projections(contents.rules(&guard), &guard, &mut projection.all_rules);

    let document_stylesheet = DocumentStyleSheet::new(stylesheet.clone());
    let custom_media = CustomMediaMap::default();
    if document_stylesheet.enabled()
        && document_stylesheet.is_effective_for_device(stylist.device(), &custom_media, &guard)
    {
        projection.effective_rule_addresses.extend(
            contents
                .effective_rules(stylist.device(), &custom_media, &guard)
                .filter_map(|rule| match rule {
                    CssRule::FontFace(rule) => Some(rule.raw_ptr().as_ptr() as usize),
                    _ => None,
                }),
        );
    }
    projection
}

pub(crate) fn native_font_face_rules_for_stylesheet(
    stylesheet: &ServoArc<Stylesheet>,
) -> Vec<NativeStylesheetFontFaceRuleProjection> {
    let guard = stylesheet.shared_lock.read();
    let mut rules = Vec::new();
    collect_font_face_rule_projections(
        stylesheet.contents(&guard).rules(&guard),
        &guard,
        &mut rules,
    );
    rules
}

fn collect_font_face_rule_projections(
    rules: &[CssRule],
    guard: &style::shared_lock::SharedRwLockReadGuard<'_>,
    projections: &mut Vec<NativeStylesheetFontFaceRuleProjection>,
) {
    for rule in rules {
        if let CssRule::FontFace(rule) = rule {
            let locked_rule = rule.read_with(guard);
            let Some(family) = locked_rule.descriptors.font_family.as_ref() else {
                continue;
            };
            let Some(source) = locked_rule.descriptors.src.as_ref() else {
                continue;
            };
            let mut serialized_source = String::new();
            if source
                .to_css(&mut CssWriter::new(&mut serialized_source))
                .is_ok()
            {
                projections.push(NativeStylesheetFontFaceRuleProjection {
                    rule: rule.clone(),
                    rule_fingerprint: locked_rule.to_css_string(guard),
                    descriptor: moli_css_parse::CssFontFace {
                        family: family.name.to_string(),
                        source: serialized_source,
                    },
                });
            }
            continue;
        }
        collect_font_face_rule_projections(rule.children(guard), guard, projections);
    }
}

pub(super) fn moli_user_agent_source_dependency_summary() -> StdArc<StyloSourceDependencySummary> {
    StdArc::clone(&MOLI_UA_SOURCE_DEPENDENCY_SUMMARY)
}

fn style_source_metadata_for_css_text_with_origin(
    css_text: &str,
    base_url: &url::Url,
    origin: Origin,
) -> StyleSourceMetadata {
    let shared_lock = SharedRwLock::new();
    let stylesheet = DocumentStyleSheet::new(ServoArc::new(parse_stylesheet(
        &shared_lock,
        base_url,
        css_text,
        origin,
        QuirksMode::NoQuirks,
    )));
    let guard = shared_lock.read();
    let stylist = new_stylist_with_viewport_bits(
        DEFAULT_VIEWPORT_WIDTH.to_bits(),
        crate::style_engine::system::DEFAULT_VIEWPORT_HEIGHT.to_bits(),
        DEFAULT_VIEWPORT_WIDTH.to_bits(),
        crate::style_engine::system::DEFAULT_VIEWPORT_HEIGHT.to_bits(),
        StyloStyleEnvironment::default(),
        QuirksMode::NoQuirks,
    );
    let mut cascade_data = CascadeData::new();
    if cascade_data
        .add_stylesheet_for_moli_source_metadata(
            stylist.device(),
            style::context::QuirksMode::NoQuirks,
            &stylesheet,
            0,
            &guard,
        )
        .is_err()
    {
        return StyleSourceMetadata::default();
    }
    style_source_metadata_from_cascade_data(&cascade_data)
}

fn style_source_metadata_from_cascade_data(cascade_data: &CascadeData) -> StyleSourceMetadata {
    let dependency_summary = StyloSourceDependencySummary::from_cascade_data(cascade_data);
    StyleSourceMetadata { dependency_summary }
}

fn new_stylist_with_viewport_bits(
    viewport_width_bits: u32,
    viewport_height_bits: u32,
    screen_width_bits: u32,
    screen_height_bits: u32,
    environment: StyloStyleEnvironment,
    quirks_mode: QuirksMode,
) -> Stylist {
    let width = f32::from_bits(viewport_width_bits);
    let height = f32::from_bits(viewport_height_bits);
    let screen_width = f32::from_bits(screen_width_bits);
    let screen_height = f32::from_bits(screen_height_bits);
    let initial_style = ComputedValues::initial_values_with_font_override(Font::initial_values());
    let mut device = Device::new(
        environment.stylo_media_type(),
        quirks_mode,
        Size2D::<f32, CSSPixel>::new(width, height),
        Size2D::<f32, DevicePixel>::new(screen_width, screen_height),
        Scale::<f32, CSSPixel, DevicePixel>::new(1.0),
        Box::new(HeadlessFontMetricsProvider),
        initial_style,
        environment.stylo_prefers_color_scheme(),
        PointerCapabilities::default(),
        PointerCapabilities::default(),
    );
    device.set_media_feature_preferences(environment.stylo_media_feature_preferences());
    Stylist::new(device, quirks_mode)
}

fn build_author_cascade_data(
    host: &DomHost,
    stylist: &mut Stylist,
    shared_lock: &SharedRwLock,
    sources: &[StyloStylesheetSource],
    quirks_mode: QuirksMode,
) -> ServoArc<style::stylist::CascadeData> {
    let stylesheets = sources
        .iter()
        .map(|source| {
            document_stylesheet_for_source(
                host,
                source,
                author_stylesheet_for_source(shared_lock, source, quirks_mode),
            )
        })
        .collect::<Vec<_>>();
    let mut author_styles = AuthorStyles::<DocumentStyleSheet>::new();
    let custom_media = CustomMediaMap::default();
    let guard = shared_lock.read();
    for stylesheet in stylesheets {
        author_styles
            .stylesheets
            .append_stylesheet(None, &custom_media, stylesheet, &guard);
    }
    author_styles.flush(stylist, &guard);
    author_styles.data
}

fn document_stylesheet_for_source(
    host: &DomHost,
    source: &StyloStylesheetSource,
    stylesheet: ServoArc<Stylesheet>,
) -> DocumentStyleSheet {
    implicit_scope_root_for_source(host, source.source_id())
        .map(|root| DocumentStyleSheet::with_implicit_scope_root(stylesheet.clone(), root))
        .unwrap_or_else(|| DocumentStyleSheet::new(stylesheet))
}

fn implicit_scope_root_for_source(
    host: &DomHost,
    source_id: Option<&StyleSourceId>,
) -> Option<ImplicitScopeRoot> {
    let source_id = source_id?;
    match &source_id.kind {
        StyleSourceKind::OwnerStyleSheet { owner }
        | StyleSourceKind::LinkedStyleSheet { owner } => {
            implicit_scope_root_for_stylesheet_owner(host, *owner)
        }
        StyleSourceKind::DocumentAdoptedStyleSheet { .. } => Some(ImplicitScopeRoot::Constructed),
        StyleSourceKind::ShadowRootAdoptedStyleSheet { .. } => Some(ImplicitScopeRoot::Constructed),
    }
}

fn implicit_scope_root_for_stylesheet_owner(
    host: &DomHost,
    owner: crate::document_runtime::DomHandle,
) -> Option<ImplicitScopeRoot> {
    let parent = host.node(owner)?.parent_node()?;
    if host.is_shadow_root(parent) {
        return host
            .shadow_root_host(parent)
            .and_then(|host_element| opaque_element_for_handle(host, host_element))
            .map(ImplicitScopeRoot::ShadowHost);
    }
    let parent_node = host.node(parent)?;
    parent_node.as_element()?;
    let opaque_parent = opaque_element_for_handle(host, parent)?;
    if host.containing_shadow_root(parent).is_some() {
        Some(ImplicitScopeRoot::InShadowTree(opaque_parent))
    } else {
        Some(ImplicitScopeRoot::InLightTree(opaque_parent))
    }
}

fn opaque_element_for_handle(
    host: &DomHost,
    handle: crate::document_runtime::DomHandle,
) -> Option<selectors::OpaqueElement> {
    let node = host.node(handle)?;
    node.as_element()?;
    Some(selectors::OpaqueElement::from_non_null_ptr(
        NonNull::new(node as *const crate::dom::native::Node as *mut ())
            .expect("DOM node pointers are never null"),
    ))
}

fn append_stylesheet_to_stylist(
    stylist: &mut Stylist,
    shared_lock: &SharedRwLock,
    css_text: &str,
    base_url: &url::Url,
    origin: Origin,
    quirks_mode: QuirksMode,
) {
    let stylesheet = parse_stylesheet(shared_lock, base_url, css_text, origin, quirks_mode);
    let guard = shared_lock.read();
    stylist.append_stylesheet(DocumentStyleSheet::new(ServoArc::new(stylesheet)), &guard);
}

fn append_author_stylesheet_source_to_stylist(
    stylist: &mut Stylist,
    shared_lock: &SharedRwLock,
    host: &DomHost,
    source: &StyloStylesheetSource,
    quirks_mode: QuirksMode,
) {
    let stylesheet = author_stylesheet_for_source(shared_lock, source, quirks_mode);
    let stylesheet = document_stylesheet_for_source(host, source, stylesheet);
    let guard = shared_lock.read();
    stylist.append_stylesheet(stylesheet, &guard);
}

fn author_stylesheet_for_source(
    shared_lock: &SharedRwLock,
    source: &StyloStylesheetSource,
    quirks_mode: QuirksMode,
) -> ServoArc<Stylesheet> {
    if let Some(stylesheet) = source.parsed_stylesheet() {
        return stylesheet;
    }
    #[cfg(test)]
    AUTHOR_SOURCE_TEXT_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    let css_text = source
        .input_css_text()
        .expect("stylesheet without parsed contents must remain text-backed");
    ServoArc::new(parse_stylesheet(
        shared_lock,
        source.base_url(),
        css_text,
        Origin::Author,
        quirks_mode,
    ))
}

fn parse_stylesheet(
    shared_lock: &SharedRwLock,
    base_url: &url::Url,
    css_text: &str,
    origin: Origin,
    quirks_mode: QuirksMode,
) -> Stylesheet {
    let media = ServoArc::new(shared_lock.wrap(MediaList::empty()));
    Stylesheet::from_str(
        css_text,
        UrlExtraData::from(base_url.clone()),
        origin,
        media,
        shared_lock.clone(),
        None,
        None,
        quirks_mode,
        AllowImportRules::No,
    )
}

impl FontMetricsProvider for HeadlessFontMetricsProvider {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        font: &Font,
        base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> FontMetrics {
        let mut metrics = FontMetrics::default();
        if font_family_list_starts_with_ahem(font) {
            metrics.zero_advance_measure = Some(base_size);
        }
        metrics
    }

    fn base_size_for_generic(&self, _generic: GenericFontFamily) -> Length {
        Length::new(16.0)
    }
}

fn font_family_list_starts_with_ahem(font: &Font) -> bool {
    font.clone_font_family()
        .families
        .iter()
        .next()
        .is_some_and(|family| match family {
            SingleFontFamily::FamilyName(name) => name.name.as_ref().eq_ignore_ascii_case("Ahem"),
            SingleFontFamily::Generic(_) => false,
        })
}
