use super::*;

pub(in crate::context_bootstrap) fn text_length(
    scope: &mut v8::PinScope<'_, '_>,
    node: v8::Local<'_, v8::Object>,
) -> u32 {
    object_string_property(scope, node, "textContent")
        .unwrap_or_default()
        .chars()
        .count() as u32
}

pub(in crate::context_bootstrap) fn first_text_descendant<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    if object_number_property(scope, node, "nodeType").unwrap_or(0.0) as u32 == 3 {
        return Some(node);
    }
    let child_nodes = object_property_as_object(scope, node, "childNodes")?;
    let length = object_number_property(scope, child_nodes, "length")? as u32;
    for index in 0..length {
        let child = child_nodes.get_index(scope, index)?;
        let child = v8::Local::<v8::Object>::try_from(child).ok()?;
        if let Some(found) = first_text_descendant(scope, child) {
            return Some(found);
        }
    }
    None
}

pub(in crate::context_bootstrap) fn last_text_descendant<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    if object_number_property(scope, node, "nodeType").unwrap_or(0.0) as u32 == 3 {
        return Some(node);
    }
    let child_nodes = object_property_as_object(scope, node, "childNodes")?;
    let length = object_number_property(scope, child_nodes, "length")? as u32;
    for index in (0..length).rev() {
        let child = child_nodes.get_index(scope, index)?;
        let child = v8::Local::<v8::Object>::try_from(child).ok()?;
        if let Some(found) = last_text_descendant(scope, child) {
            return Some(found);
        }
    }
    None
}
