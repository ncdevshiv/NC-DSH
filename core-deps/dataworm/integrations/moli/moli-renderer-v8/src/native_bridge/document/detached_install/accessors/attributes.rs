use super::super::super::{
    detached_attribute_name, detached_attributes_map, detached_map_get, detached_map_has,
    detached_map_set, read_detached_native_attribute, read_detached_native_has_attribute,
    with_detached_native_element_reaction_scope,
    write_detached_native_attribute_appending_to_current_reaction_queue,
};

pub(super) fn detached_element_attribute_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    name: &str,
) -> Option<String> {
    let normalized = detached_attribute_name(scope, element, name);
    if let Some(has_attribute) = read_detached_native_has_attribute(scope, element, &normalized) {
        return if has_attribute {
            read_detached_native_attribute(scope, element, &normalized)
        } else {
            None
        };
    }
    let attributes = detached_attributes_map(scope, element)?;
    detached_map_get(scope, attributes, &normalized)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

pub(super) fn detached_element_has_attribute<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    name: &str,
) -> bool {
    let normalized = detached_attribute_name(scope, element, name);
    if let Some(has_attribute) = read_detached_native_has_attribute(scope, element, &normalized) {
        return has_attribute;
    }
    detached_attributes_map(scope, element)
        .is_some_and(|attributes| detached_map_has(scope, attributes, &normalized))
}

pub(super) fn set_detached_element_attribute_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    name: &str,
    value: v8::Local<'s, v8::Value>,
) -> bool {
    let normalized = detached_attribute_name(scope, element, name);
    let Some(value) = value.to_string(scope) else {
        return false;
    };
    let value = value.to_rust_string_lossy(scope);
    if with_detached_native_element_reaction_scope(scope, element, |scope| {
        write_detached_native_attribute_appending_to_current_reaction_queue(
            scope,
            element,
            &normalized,
            &value,
        )
    })
    .unwrap_or(false)
    {
        return true;
    }
    let Some(attributes) = detached_attributes_map(scope, element) else {
        return false;
    };
    detached_map_set(scope, attributes, &normalized, &value);
    true
}
