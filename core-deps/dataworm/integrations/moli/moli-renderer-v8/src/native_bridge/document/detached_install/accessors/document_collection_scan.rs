use crate::{
    native_bridge::document::{
        build_detached_html_collection, call_object_method, detached_document_element_object,
        detached_element_local_name, detached_native_handle, detached_native_object_for_handle,
        read_detached_native_has_attribute,
    },
    util::{context_host_ptr_from_global_bridge, v8_string},
};

use super::document_tree_scan::collect_detached_elements;

pub(in crate::native_bridge::document) fn detached_document_collection_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
    mut matches: impl FnMut(&mut v8::PinScope<'s, '_>, v8::Local<'s, v8::Object>) -> bool,
) -> Option<v8::Local<'s, v8::Object>> {
    if let Some(values) = detached_native_document_collection(scope, document, &mut matches) {
        return build_detached_html_collection(scope, &values);
    }
    let mut values = Vec::new();
    if let Some(root) = detached_document_element_object(scope, document) {
        collect_detached_elements(scope, root, &mut values, &mut matches);
    }
    build_detached_html_collection(scope, &values)
}

fn detached_native_document_collection<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
    matches: &mut impl FnMut(&mut v8::PinScope<'s, '_>, v8::Local<'s, v8::Object>) -> bool,
) -> Option<Vec<v8::Local<'s, v8::Object>>> {
    let runtime_ptr = context_host_ptr_from_global_bridge(scope)?;
    let root = detached_native_handle(scope, document)?;
    let handles = {
        let dom_host = unsafe { &*runtime_ptr }.dom_host();
        let mut stack = dom_host.child_handles_reversed(root).collect::<Vec<_>>();
        let mut handles = Vec::new();
        while let Some(handle) = stack.pop() {
            if dom_host
                .node(handle)
                .and_then(crate::dom::native::Node::as_element)
                .is_some()
            {
                handles.push(handle);
            }
            stack.extend(dom_host.child_handles_reversed(handle));
        }
        handles
    };
    let mut out = Vec::new();
    for handle in handles {
        let Some(node) = detached_native_object_for_handle(scope, runtime_ptr, handle) else {
            continue;
        };
        if matches(scope, node) {
            out.push(node);
        }
    }
    Some(out)
}

pub(super) fn detached_element_local_name_is<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
    expected: &str,
) -> bool {
    detached_element_local_name(scope, node).is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

pub(super) fn detached_element_has_attribute<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
    name: &str,
) -> bool {
    if let Some(has_attribute) = read_detached_native_has_attribute(scope, node, name) {
        return has_attribute;
    }
    let Some(name) = v8_string(scope, name) else {
        return false;
    };
    call_object_method(scope, node, "hasAttribute", &[name.into()])
        .is_some_and(|value| value.boolean_value(scope))
}
