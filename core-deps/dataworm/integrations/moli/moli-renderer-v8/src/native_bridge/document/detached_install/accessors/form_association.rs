use super::super::super::{detached_element_local_name, detached_parent_node_object};
use super::attributes::{detached_element_attribute_value, detached_element_has_attribute};
use super::document_tree_scan::find_detached_element;

pub(in crate::native_bridge) fn detached_label_control_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    label: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    if !detached_element_local_name_is(scope, label, "label") {
        return None;
    }
    if let Some(for_id) =
        detached_element_attribute_value(scope, label, "for").filter(|value| !value.is_empty())
    {
        let root = detached_tree_root_object(scope, label);
        let candidate = find_detached_element_by_id(scope, root, &for_id)?;
        return detached_is_labelable_element(scope, candidate).then_some(candidate);
    }
    find_detached_element(scope, label, &mut |scope, candidate| {
        detached_is_labelable_element(scope, candidate)
    })
}

pub(in crate::native_bridge) fn detached_form_owner_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    control: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    detached_form_owner(scope, control)
}

fn detached_form_owner<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    control: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    if !detached_is_form_associated_element(scope, control) {
        return None;
    }
    if detached_element_has_attribute(scope, control, "form") {
        let form_id = detached_element_attribute_value(scope, control, "form")?;
        let root = detached_tree_root_object(scope, control);
        let candidate = find_detached_element_by_id(scope, root, &form_id)?;
        return detached_element_local_name_is(scope, candidate, "form").then_some(candidate);
    }
    let mut current = detached_parent_node_object(scope, control);
    while let Some(candidate) = current {
        if detached_element_local_name_is(scope, candidate, "form") {
            return Some(candidate);
        }
        current = detached_parent_node_object(scope, candidate);
    }
    None
}

fn detached_tree_root_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> v8::Local<'s, v8::Object> {
    let mut current = node;
    while let Some(parent) = detached_parent_node_object(scope, current) {
        current = parent;
    }
    current
}

fn find_detached_element_by_id<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    root: v8::Local<'s, v8::Object>,
    id: &str,
) -> Option<v8::Local<'s, v8::Object>> {
    if id.is_empty() {
        return None;
    }
    find_detached_element(scope, root, &mut |scope, candidate| {
        detached_element_attribute_value(scope, candidate, "id").as_deref() == Some(id)
    })
}

fn detached_is_labelable_element<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> bool {
    match detached_element_local_name(scope, element).as_deref() {
        Some("button" | "meter" | "output" | "progress" | "select" | "textarea") => true,
        Some("input") => detached_element_attribute_value(scope, element, "type")
            .is_none_or(|input_type| !input_type.eq_ignore_ascii_case("hidden")),
        _ => false,
    }
}

fn detached_is_form_associated_element<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> bool {
    matches!(
        detached_element_local_name(scope, element).as_deref(),
        Some("button" | "fieldset" | "input" | "object" | "output" | "select" | "textarea")
    )
}

fn detached_element_local_name_is<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    expected: &str,
) -> bool {
    detached_element_local_name(scope, element)
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}
