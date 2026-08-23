use super::*;

pub(in crate::context_bootstrap) fn dom_node_contains_internal<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
    other: v8::Local<'s, v8::Object>,
) -> bool {
    let Some(host_ptr) = context_host_ptr_from_global_bridge(scope) else {
        return false;
    };
    let Some(node_handle) = callback_value_dom_handle(scope, node.into()) else {
        return false;
    };
    let Some(other_handle) = callback_value_dom_handle(scope, other.into()) else {
        return false;
    };
    let host = unsafe { &*host_ptr };
    host.dom_host()
        .node(node_handle)
        .is_some_and(|node| node.contains(host.dom_host().dom(), other_handle))
}

pub(in crate::context_bootstrap) fn node_owner_document_or_self<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    let node_type = object_number_property(scope, node, "nodeType")? as u32;
    if node_type == 9 {
        return Some(node);
    }
    object_property_as_object(scope, node, "ownerDocument")
}
