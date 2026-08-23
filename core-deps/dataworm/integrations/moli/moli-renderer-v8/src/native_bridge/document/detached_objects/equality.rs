use super::*;
use crate::util::context_host_ptr_from_global_bridge;

fn detached_read_child_node_objects<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Vec<v8::Local<'s, v8::Object>> {
    detached_child_node_objects(scope, node)
}

fn detached_native_nodes_equal<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    left: v8::Local<'s, v8::Object>,
    right: v8::Local<'s, v8::Object>,
) -> Option<bool> {
    let runtime_ptr = context_host_ptr_from_global_bridge(scope)?;
    let left_handle = detached_native_handle_for_runtime(scope, runtime_ptr, left)?;
    let right_handle = detached_native_handle_for_runtime(scope, runtime_ptr, right)?;
    let dom_host = unsafe { &*runtime_ptr }.dom_host();
    let left_node = dom_host.node(left_handle)?;
    let right_node = dom_host.node(right_handle)?;
    Some(left_node.is_equal_node(dom_host, right_node))
}

struct DetachedAttributeSnapshot {
    names: Vec<String>,
    native_backed: bool,
}

fn detached_attribute_snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
) -> DetachedAttributeSnapshot {
    if let Some(names) = read_detached_native_attribute_names(scope, element) {
        return DetachedAttributeSnapshot {
            names,
            native_backed: true,
        };
    }

    let names = call_object_method(scope, element, "getAttributeNames", &[])
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
        .map(|names| {
            let mut out = Vec::new();
            let length = names
                .get(scope, v8str(scope, "length").into())
                .and_then(|value| value.uint32_value(scope))
                .unwrap_or(0);
            for index in 0..length {
                let Some(name) = names.get_index(scope, index) else {
                    continue;
                };
                let Some(name) = name.to_string(scope) else {
                    continue;
                };
                out.push(name.to_rust_string_lossy(scope));
            }
            out
        })
        .unwrap_or_default();

    DetachedAttributeSnapshot {
        names,
        native_backed: false,
    }
}

fn detached_attribute_snapshot_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    element: v8::Local<'s, v8::Object>,
    snapshot: &DetachedAttributeSnapshot,
    name: &str,
) -> Option<String> {
    if snapshot.native_backed {
        return read_detached_native_attribute(scope, element, name);
    }

    let name_value = v8_string(scope, name)?;
    call_object_method(scope, element, "getAttribute", &[name_value.into()])
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

pub(in crate::native_bridge::document) fn detached_nodes_equal<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    left: v8::Local<'s, v8::Object>,
    right: v8::Local<'s, v8::Object>,
) -> bool {
    if let Some(equal) = detached_native_nodes_equal(scope, left, right) {
        return equal;
    }

    let left_type = detached_node_type(scope, left);
    let right_type = detached_node_type(scope, right);
    if left_type.is_none() || right_type.is_none() {
        return left == right;
    }
    if left_type != right_type
        || object_string_property(scope, left, "nodeName")
            != object_string_property(scope, right, "nodeName")
    {
        return false;
    }

    match left_type.unwrap_or_default() {
        10 => {
            return object_string_property(scope, left, "name")
                == object_string_property(scope, right, "name")
                && object_string_property(scope, left, "publicId")
                    == object_string_property(scope, right, "publicId")
                && object_string_property(scope, left, "systemId")
                    == object_string_property(scope, right, "systemId");
        }
        3 | 8 | 7 => {
            return detached_character_data_value(scope, left)
                == detached_character_data_value(scope, right);
        }
        1 => {
            let left_attributes = detached_attribute_snapshot(scope, left);
            let right_attributes = detached_attribute_snapshot(scope, right);
            if left_attributes.names.len() != right_attributes.names.len() {
                return false;
            }
            for name in &left_attributes.names {
                let left_value =
                    detached_attribute_snapshot_value(scope, left, &left_attributes, name);
                let right_value =
                    detached_attribute_snapshot_value(scope, right, &right_attributes, name);
                if left_value != right_value {
                    return false;
                }
            }
        }
        _ => {}
    }

    let left_children = detached_read_child_node_objects(scope, left);
    let right_children = detached_read_child_node_objects(scope, right);
    if left_children.len() != right_children.len() {
        return false;
    }
    for (left_child, right_child) in left_children.into_iter().zip(right_children) {
        if !detached_nodes_equal(scope, left_child, right_child) {
            return false;
        }
    }
    true
}

pub(in crate::native_bridge::document) fn detached_text_content<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    match detached_node_type(scope, node) {
        Some(9 | 10) => None,
        Some(3 | 4 | 8 | 7) => Some(detached_character_data_value(scope, node)),
        Some(_) => Some(
            detached_read_child_node_objects(scope, node)
                .into_iter()
                .fold(String::new(), |mut out, child| {
                    append_detached_text_content(scope, child, &mut out);
                    out
                }),
        ),
        None => Some(String::new()),
    }
}

fn append_detached_text_content<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
    out: &mut String,
) {
    match detached_node_type(scope, node) {
        Some(3 | 4) => out.push_str(&detached_character_data_value(scope, node)),
        Some(7..=10) | None => {}
        Some(_) => {
            for child in detached_read_child_node_objects(scope, node) {
                append_detached_text_content(scope, child, out);
            }
        }
    }
}
