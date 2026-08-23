use super::document_collection_scan::{
    detached_document_collection_value, detached_element_has_attribute,
    detached_element_local_name_is,
};

pub(in crate::native_bridge::document) fn detached_document_images_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        detached_element_local_name_is(scope, node, "img")
    })
}

pub(in crate::native_bridge::document) fn detached_document_embeds_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        detached_element_local_name_is(scope, node, "embed")
    })
}

pub(in crate::native_bridge::document) fn detached_document_links_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        (detached_element_local_name_is(scope, node, "a")
            || detached_element_local_name_is(scope, node, "area"))
            && detached_element_has_attribute(scope, node, "href")
    })
}

pub(in crate::native_bridge::document) fn detached_document_forms_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        detached_element_local_name_is(scope, node, "form")
    })
}

pub(in crate::native_bridge::document) fn detached_document_scripts_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        detached_element_local_name_is(scope, node, "script")
    })
}

pub(in crate::native_bridge::document) fn detached_document_anchors_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |scope, node| {
        detached_element_local_name_is(scope, node, "a")
            && detached_element_has_attribute(scope, node, "name")
    })
}

pub(in crate::native_bridge::document) fn detached_document_applets_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_document_collection_value(scope, document, |_scope, _node| false)
}
