use super::super::*;
use crate::native_bridge::document::{
    detached_character_data_value, detached_child_node_objects, detached_element_local_name,
    detached_element_namespace_uri, detached_element_prefix, detached_is_node,
    detached_native_handle_for_runtime, detached_node_type,
    read_detached_native_attribute_snapshot,
};
use crate::util::context_host_ptr_from_global_bridge;
use moli_xpath::{Snapshot, SnapshotBuilder, SnapshotNodeId};

use super::super::super::attributes::native_attr_object_from_snapshot;

enum DetachedXPathNativeNodeInfo {
    Element {
        tag_name: String,
        prefix: Option<String>,
        namespace: Option<String>,
        is_html_element: bool,
    },
    Text(String),
    Comment(String),
    Container,
    Unsupported,
}

fn detached_xpath_node_type<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<i32> {
    detached_node_type(scope, node)
}

fn detached_xpath_native_node_info<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<DetachedXPathNativeNodeInfo> {
    let runtime_ptr = context_host_ptr_from_global_bridge(scope)?;
    let handle = detached_native_handle_for_runtime(scope, runtime_ptr, node)?;
    let dom_host = unsafe { &*runtime_ptr }.dom_host();
    let node = dom_host.node(handle)?;
    let info = match node.data() {
        crate::dom::native::NodeData::Element(element) => {
            let namespace =
                (!element.namespace().is_empty()).then(|| element.namespace().to_owned());
            DetachedXPathNativeNodeInfo::Element {
                tag_name: element.local_name().to_owned(),
                prefix: element.prefix().map(str::to_owned),
                is_html_element: namespace
                    .as_deref()
                    .is_none_or(|namespace| namespace == "http://www.w3.org/1999/xhtml"),
                namespace,
            }
        }
        crate::dom::native::NodeData::Text(text) => {
            DetachedXPathNativeNodeInfo::Text(text.data().to_owned())
        }
        crate::dom::native::NodeData::CDataSection(cdata) => {
            DetachedXPathNativeNodeInfo::Text(cdata.data().to_owned())
        }
        crate::dom::native::NodeData::Comment(comment) => {
            DetachedXPathNativeNodeInfo::Comment(comment.data().to_owned())
        }
        crate::dom::native::NodeData::Document(_)
        | crate::dom::native::NodeData::DocumentFragment(_) => {
            DetachedXPathNativeNodeInfo::Container
        }
        crate::dom::native::NodeData::DocumentType(_)
        | crate::dom::native::NodeData::ProcessingInstruction(_) => {
            DetachedXPathNativeNodeInfo::Unsupported
        }
    };
    Some(info)
}

pub(super) struct XPathSnapshot<'s> {
    pub(super) snapshot: Snapshot,
    pub(super) root_id: SnapshotNodeId,
    pub(super) original_nodes: Vec<Option<v8::Local<'s, v8::Object>>>,
}

fn detached_xpath_element_attribute_names<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Vec<String> {
    let Some(names) = call_object_method(scope, node, "getAttributeNames", &[]) else {
        return Vec::new();
    };
    let Ok(names) = v8::Local::<v8::Object>::try_from(names) else {
        return Vec::new();
    };
    let Some(length) =
        object_property_value(scope, names, "length").and_then(|value| value.uint32_value(scope))
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(length as usize);
    for index in 0..length {
        let Some(value) = names.get_index(scope, index) else {
            continue;
        };
        let Some(value) = value.to_string(scope) else {
            continue;
        };
        out.push(value.to_rust_string_lossy(scope));
    }
    out
}

fn detached_xpath_element_tag_name<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Option<String> {
    detached_element_local_name(scope, node).or_else(|| {
        if detached_is_node(scope, node) {
            return None;
        }
        object_string_property(scope, node, "localName").or_else(|| {
            object_string_property(scope, node, "nodeName").map(|name| name.to_ascii_lowercase())
        })
    })
}

fn detached_xpath_child_nodes<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> Vec<v8::Local<'s, v8::Object>> {
    detached_child_node_objects(scope, node)
}

pub(super) fn build_xpath_snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    root: v8::Local<'s, v8::Object>,
) -> Option<XPathSnapshot<'s>> {
    let root_node_type = detached_xpath_node_type(scope, root).unwrap_or_default();
    if root_node_type != 9 && root_node_type != 1 {
        return None;
    }

    let mut builder = SnapshotBuilder::new();
    let document_id = builder.append_document();
    let mut original_nodes = vec![None];
    let root_id = if root_node_type == 9 {
        original_nodes[document_id] = Some(root);
        for child in detached_xpath_child_nodes(scope, root) {
            append_xpath_snapshot_node(
                scope,
                &mut builder,
                document_id,
                child,
                &mut original_nodes,
            );
        }
        document_id
    } else {
        append_xpath_snapshot_node(scope, &mut builder, document_id, root, &mut original_nodes)?
    };

    Some(XPathSnapshot {
        snapshot: builder.finish(document_id),
        root_id,
        original_nodes,
    })
}

fn append_xpath_snapshot_node<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    builder: &mut SnapshotBuilder,
    parent_id: SnapshotNodeId,
    node: v8::Local<'s, v8::Object>,
    original_nodes: &mut Vec<Option<v8::Local<'s, v8::Object>>>,
) -> Option<SnapshotNodeId> {
    if let Some(info) = detached_xpath_native_node_info(scope, node) {
        return append_xpath_native_snapshot_node(
            scope,
            builder,
            parent_id,
            node,
            info,
            original_nodes,
        );
    }

    match detached_xpath_node_type(scope, node) {
        Some(1) => append_xpath_snapshot_element(scope, builder, parent_id, node, original_nodes),
        Some(3) => {
            let text = detached_xpath_character_data_value(scope, node);
            let id = builder.append_text(parent_id, text);
            push_original_node(original_nodes, id, Some(node));
            Some(id)
        }
        Some(8) => {
            let text = detached_xpath_character_data_value(scope, node);
            let id = builder.append_comment(parent_id, text);
            push_original_node(original_nodes, id, Some(node));
            Some(id)
        }
        Some(9) | Some(11) => {
            for child in detached_xpath_child_nodes(scope, node) {
                append_xpath_snapshot_node(scope, builder, parent_id, child, original_nodes);
            }
            None
        }
        _ => None,
    }
}

fn detached_xpath_character_data_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: v8::Local<'s, v8::Object>,
) -> String {
    if detached_is_node(scope, node) {
        return detached_character_data_value(scope, node);
    }
    object_string_property(scope, node, "nodeValue").unwrap_or_default()
}

fn append_xpath_native_snapshot_node<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    builder: &mut SnapshotBuilder,
    parent_id: SnapshotNodeId,
    node: v8::Local<'s, v8::Object>,
    info: DetachedXPathNativeNodeInfo,
    original_nodes: &mut Vec<Option<v8::Local<'s, v8::Object>>>,
) -> Option<SnapshotNodeId> {
    match info {
        DetachedXPathNativeNodeInfo::Element {
            tag_name,
            prefix,
            namespace,
            is_html_element,
        } => append_xpath_native_snapshot_element(
            scope,
            builder,
            parent_id,
            node,
            tag_name,
            prefix,
            namespace,
            is_html_element,
            original_nodes,
        ),
        DetachedXPathNativeNodeInfo::Text(text) => {
            let id = builder.append_text(parent_id, text);
            push_original_node(original_nodes, id, Some(node));
            Some(id)
        }
        DetachedXPathNativeNodeInfo::Comment(text) => {
            let id = builder.append_comment(parent_id, text);
            push_original_node(original_nodes, id, Some(node));
            Some(id)
        }
        DetachedXPathNativeNodeInfo::Container => {
            for child in detached_xpath_child_nodes(scope, node) {
                append_xpath_snapshot_node(scope, builder, parent_id, child, original_nodes);
            }
            None
        }
        DetachedXPathNativeNodeInfo::Unsupported => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn append_xpath_native_snapshot_element<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    builder: &mut SnapshotBuilder,
    parent_id: SnapshotNodeId,
    node: v8::Local<'s, v8::Object>,
    tag_name: String,
    prefix: Option<String>,
    namespace: Option<String>,
    is_html_element: bool,
    original_nodes: &mut Vec<Option<v8::Local<'s, v8::Object>>>,
) -> Option<SnapshotNodeId> {
    let id = builder.append_element(parent_id, tag_name, prefix, namespace, is_html_element);
    push_original_node(original_nodes, id, Some(node));

    for attribute in read_detached_native_attribute_snapshot(scope, node).unwrap_or_default() {
        let attr_object = native_attr_object_from_snapshot(scope, node, &attribute);
        let attr_id = builder.append_attribute(
            id,
            attribute.local_name,
            attribute.value,
            attribute.prefix,
            attribute.namespace_uri,
        );
        push_original_node(original_nodes, attr_id, attr_object);
    }

    for child in detached_xpath_child_nodes(scope, node) {
        append_xpath_snapshot_node(scope, builder, id, child, original_nodes);
    }
    Some(id)
}

fn append_xpath_snapshot_element<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    builder: &mut SnapshotBuilder,
    parent_id: SnapshotNodeId,
    node: v8::Local<'s, v8::Object>,
    original_nodes: &mut Vec<Option<v8::Local<'s, v8::Object>>>,
) -> Option<SnapshotNodeId> {
    let tag_name = detached_xpath_element_tag_name(scope, node)?;
    let prefix = detached_element_prefix(scope, node).or_else(|| {
        (!detached_is_node(scope, node)).then(|| object_string_property(scope, node, "prefix"))?
    });
    let namespace = detached_element_namespace_uri(scope, node).or_else(|| {
        (!detached_is_node(scope, node))
            .then(|| object_string_property(scope, node, "namespaceURI"))?
    });
    let is_html_element = namespace
        .as_deref()
        .is_none_or(|namespace| namespace == "http://www.w3.org/1999/xhtml");
    let id = builder.append_element(parent_id, tag_name, prefix, namespace, is_html_element);
    push_original_node(original_nodes, id, Some(node));

    for name in detached_xpath_element_attribute_names(scope, node) {
        let Some(name_value) = v8_string(scope, &name) else {
            continue;
        };
        let Some(value) = call_object_method(scope, node, "getAttribute", &[name_value.into()])
        else {
            continue;
        };
        let Some(value) = value.to_string(scope) else {
            continue;
        };
        let attr_object = call_object_method(scope, node, "getAttributeNode", &[name_value.into()])
            .and_then(|value| {
                if value.is_null_or_undefined() {
                    None
                } else {
                    v8::Local::<v8::Object>::try_from(value).ok()
                }
            });
        let attr_id =
            builder.append_attribute(id, name, value.to_rust_string_lossy(scope), None, None);
        push_original_node(original_nodes, attr_id, attr_object);
    }

    for child in detached_xpath_child_nodes(scope, node) {
        append_xpath_snapshot_node(scope, builder, id, child, original_nodes);
    }
    Some(id)
}

fn push_original_node<'s>(
    original_nodes: &mut Vec<Option<v8::Local<'s, v8::Object>>>,
    id: SnapshotNodeId,
    node: Option<v8::Local<'s, v8::Object>>,
) {
    while original_nodes.len() <= id {
        original_nodes.push(None);
    }
    original_nodes[id] = node;
}

pub(super) fn xpath_context_node_id<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context_node: v8::Local<'s, v8::Object>,
    snapshot: &XPathSnapshot<'s>,
) -> Option<SnapshotNodeId> {
    if detached_xpath_node_type(scope, context_node) == Some(9) {
        return Some(snapshot.root_id);
    }

    let context_identity = object_dom_identity(scope, context_node);
    snapshot
        .original_nodes
        .iter()
        .enumerate()
        .find_map(|(id, candidate)| {
            let candidate = *candidate.as_ref()?;
            (candidate.strict_equals(context_node.into())
                || (object_dom_identity(scope, candidate).is_some()
                    && object_dom_identity(scope, candidate) == context_identity))
                .then_some(id)
        })
}
