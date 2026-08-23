use super::*;

pub(in crate::native_bridge) fn build_collection_wrapper_template<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let template = v8::ObjectTemplate::new(scope);
    let _ = template.set_internal_field_count(3);
    template
}

pub(in crate::native_bridge) fn build_static_handle_node_list_wrapper_template<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let template = v8::ObjectTemplate::new(scope);
    assert!(
        template.set_internal_field_count(4),
        "handle-backed static NodeList template must expose four internal fields"
    );
    template.set_indexed_property_handler(
        v8::IndexedPropertyHandlerConfiguration::new()
            .getter(static_handle_collection_indexed_getter)
            .setter(static_handle_collection_indexed_setter)
            .query(static_handle_collection_indexed_query)
            .deleter(static_handle_collection_indexed_deleter)
            .enumerator(static_handle_collection_indexed_enumerator)
            .definer(static_handle_collection_indexed_definer)
            .descriptor(static_handle_collection_indexed_descriptor),
    );
    template
}

pub(in crate::native_bridge) fn build_live_collection_wrapper_template<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let template = v8::ObjectTemplate::new(scope);
    let _ = template.set_internal_field_count(2);
    template.set_indexed_property_handler(
        v8::IndexedPropertyHandlerConfiguration::new()
            .getter(live_collection_indexed_getter)
            .setter(live_collection_indexed_setter)
            .query(live_collection_indexed_query)
            .deleter(live_collection_indexed_deleter)
            .enumerator(live_collection_indexed_enumerator)
            .definer(live_collection_indexed_definer)
            .descriptor(live_collection_indexed_descriptor),
    );
    template.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(live_collection_named_getter)
            .setter(live_collection_named_setter)
            .query(live_collection_named_query)
            .deleter(live_collection_named_deleter)
            .enumerator(live_collection_named_enumerator)
            .definer(live_collection_named_definer)
            .descriptor(live_collection_named_descriptor)
            .flags(
                v8::PropertyHandlerFlags::NON_MASKING
                    | v8::PropertyHandlerFlags::ONLY_INTERCEPT_STRINGS,
            ),
    );
    template
}
