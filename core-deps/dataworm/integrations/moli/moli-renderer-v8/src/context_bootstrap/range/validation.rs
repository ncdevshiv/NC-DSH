use super::*;

pub(in crate::context_bootstrap) fn range_validate_boundary_point<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    container: v8::Local<'s, v8::Object>,
    offset: u32,
) -> bool {
    range_node_length(scope, container).is_some_and(|length| offset <= length)
}

pub(in crate::context_bootstrap) fn range_node_length<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<u32> {
    let node_type = object_number_property(scope, node, "nodeType")? as u32;
    match node_type {
        3 | 4 | 7 | 8 => Some(object_number_property(scope, node, "length")? as u32),
        _ => {
            let child_nodes = object_property_as_object(scope, node, "childNodes")?;
            Some(object_number_property(scope, child_nodes, "length")? as u32)
        }
    }
}

pub(in crate::context_bootstrap) fn child_index<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    parent: v8::Local<'s, v8::Object>,
    child: v8::Local<'s, v8::Object>,
) -> Option<u32> {
    // Verify `child` actually belongs to `parent` cheaply via parentNode, then
    // count previousSibling steps. This is O(index) JS-property fetches, which
    // beats the previous O(parent.childNodes.length) loop both in the common
    // case (small index) and when the childNodes proxy is expensive to walk.
    let actual_parent = object_property_as_object(scope, child, "parentNode")?;
    if !actual_parent.strict_equals(parent.into()) {
        return None;
    }
    let mut index: u32 = 0;
    let mut current = child;
    while let Some(prev) = object_property_as_object(scope, current, "previousSibling") {
        index += 1;
        current = prev;
    }
    Some(index)
}
